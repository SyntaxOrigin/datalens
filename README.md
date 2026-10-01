# VeriMercek / DataLens

Büyük CSV, TSV ve JSONL dosyalarını **belleğe almadan** açan, profilleyen, filtreleyen ve
dışa aktaran terminal görüntüleyicisi.

Temel fikir: geleneksel tablo araçları veriyi önce belleğe yükler. DataLens bunu tersine
çevirir. Dosya **iki geçişte** okunur:

1. **İndeksleme:** dosya baştan sona, sabit 256 KiB'lik bir tamponla akış hâlinde taranır.
   Her kaydın `(başlangıç_baytı, uzunluk)` aralığı saklanır. Tampon boyutu sabittir;
   dosya boyutundan bağımsızdır.
2. **Veri okuma:** yalnızca istenen satırlar `File::seek` ile **konum bazlı** okunur.
   Dosya hiçbir zaman belleğe tümüyle alınmaz.

Ölçülen sonuç: **256 MB'lık, 5.932.172 satırlık bir CSV, 5,0 MB tepe çalışma setiyle
açılıyor** (varsayılan *seyrek* indeks kipi). Ayrıntı için [Sabit bellek](#sabit-bellek)
bölümüne bakın.

---

## Özellikler

MVP kapsamındaki her özellik ayrı madde olarak:

- **Bayt aralıklı satır indeksi.** Her kayıt için `(başlangıç, uzunluk)` çifti; dosyanın
  tamamı belleğe alınmaz. Rapordaki (b05, b07) çekirdek mimarinin doğrudan uygulaması.
- **İki indeks kipi.**
  - `seyrek` (varsayılan): her 512 kayıt için 8 bayt. İndeks belleği dosya boyutundan
    bağımsızdır. 256 MB dosyada 92.696 bayt.
  - `yogun`: her kayıt için 16 bayt. Rastgele erişim tek `seek` + tek `read` ile yapılır,
    ancak bellek satır sayısıyla artar (256 MB dosyada 94.914.768 bayt).
- **CSV ayrıştırıcı (RFC 4180 + üstü).** Tırnaklı alan, `""` kaçışı, alan içi CRLF/LF,
  tırnak kapatılmamış alan, tırnaksız alan içinde tırnak, alan başında olmayan tırnağın
  veri sayılması, tırnak sonrası ek veri.
- **Özel ayraçlar.** `,` `;` `\t` `|` — ilk kayıttan otomatik algılanır, `--ayrac` ile
  elle verilebilir.
- **TSV desteği.** CSV ile aynı ayrıştırıcı; varsayılan ayraç sekme.
- **JSONL desteği.** Satır başına tek JSON değeri; sütun adları ilk 200 kaydın anahtar
  birleşiminden türetilir. **JSON dizi kökü** (`[{...},{...}]`) de desteklenir — her eleman
  bir kayıttır.
- **Sütun tipi çıkarımı.** `bos`, `mantiksal`, `tam_sayi`, `kayan_nokta`, `tarih`, `metin`.
  Tipler toplam sıralama üzerinde yükseltilir: `Bos < Mantiksal < TamSayi < KayanNokta <
  Tarih < Metin`.
- **Boş değer oranı** ve boş/dolu sayacı.
- **Şema ve profil çıkarımı.** Satır sayısı, sütun sayısı, sütun başına sayısal
  min/max/ortalama, tarih min/max, en sık 5 değer, ayrık değer sayısı, metin biçim
  varyantı sayısı, "tarih gibi görünüp geçersiz" değer sayısı. Çıktı JSON.
- **Filtreleme.** Dört operatör: `=`, `>`, `<`, `contains`. Birden çok koşul verilebilir ve
  hepsi birlikte (VE) uygulanır. Sonuç **satır numarası listesi** olarak döner; satır
  verisi kopyalanmaz. Eşleşen satır sayısı ve yüzdesi yazılır.
- **Sanal kaydırma penceresi.** `sample --baslangic N --adet M` ile dosyanın istenen
  aralığı okunur; yalnızca o satırlar basılır.
- **Dışa aktarım.** CSV (RFC 4180, `CRLF`) ve JSONL. JSONL kaynaktan gelen satırlar özgün
  JSON değerlerinden yeniden üretilir, böylece sayı/boolean/iç içe nesne tipleri korunur.
  Yazım **atomiktir**: hedefin aynı dizininde geçici adla açılır, hata olursa silinir.
- **JSON çıktısı** (`serde_json`) — `schema` ve `count` komutları doğrudan JSON üretir.
- **Satır hatası kaybı yok.** Bozuk JSON, eksik sütun, geçersiz UTF-8, tırnak hatası
  satırı düşürmez; satır işaretlenir ve uyarı metni çıktının altında listelenir.

---

## Kurulum

Gereksinim: Rust 1.74 veya üzeri (MSRV `rust-version = "1.74"`). Harici çalışma zamanı,
paket yöneticisi veya sistem kitaplığı gerekmez.

```bash
cargo build --release
```

Üretilen tek dosya: `target/release/datalens.exe` (Windows) veya
`target/release/datalens` (Linux/macOS).

Kurulum yerine geçirmek için:

```bash
cargo install --path .
```

Doğrulanmış ortam: `rustc 1.98.1`, ana makine `x86_64-pc-windows-gnu`, Windows.
MSRV 1.74 **yalnızca bildirilmiştir**; 1.74'lük bir araç zinciriyle ayrıca
doğrulanmamıştır (bkz. [Bilinen Sınırlamalar](#bilinen-sinirlamalar)).

---

## Kullanım

`ornek/` klasöründeki üç küçük dosya tüm örneklerde kullanılır; komutlar önce gerçekten
çalıştırılmış, çıktılar kopyalanmıştır.

### 1. `count` — satır/sütun sayısı ve indeks maliyeti

```bash
datalens count ornek/satislar.csv
```

```json
{
  "atlanan_bos_satir": 0,
  "dosya": {
    "ad": "satislar.csv",
    "ayrac": ",",
    "baslik_var": true,
    "bicim": "csv",
    "blok_satir": 512,
    "boyut_bayt": 555,
    "dizi_kok": false,
    "indeks_bellek_bayt": 8,
    "indeks_kipi": "seyrek",
    "yol": ".\\ornek\\satislar.csv"
  },
  "indeks_bellek_bayt": 8,
  "indeks_gecisi_bayt": 555,
  "satir_sayisi": 10,
  "sutun_sayisi": 7,
  "sutunlar": [
    "siparis_no", "urun", "adet", "fiyat", "tarih", "sehir", "not"
  ]
}
```

`indeks_gecisi_bayt` dosyanın tamamıdır (akış hâlinde, sabit tamponla okunur), ancak
`indeks_bellek_bayt` **o satırları tutmak için ayrılan** bellektir. 5.9 milyon satırda
bu sayı 92.696 bayttır.

### 2. `head` — ilk satırları tablo olarak görmek

```bash
datalens head ornek/satislar.csv --adet 4
```

```text
       # | siparis_no |           urun | adet |  fiyat |      tarih |    sehir |            not
---------+------------+----------------+------+--------+------------+----------+---------------
       0 |       1001 | Kalem, kırmızı |   12 |  45.50 | 2024-01-15 |   Ankara |
       1 |       1002 |         Defter |    3 | 120.00 | 2024-01-16 | İstanbul |          acele
       2 |       1003 |   Çanta, büyük |    1 | 899.90 | 15.02.2024 |    İzmir |
       3 |       1004 |          Kalem |   25 |  45.50 | 2024-02-20 |   Ankara | ikinci sipariş
4 / 10 satir  |  pencere 0..4  |  indeks seyrek  |  okunan 5072 bayt
```

Alt satırdaki `okunan ... bayt`, **bu pencere için gerçekten diskten okunan bayt** sayısıdır.

### 3. `sample` — sanal kaydırma penceresi

```bash
datalens sample ornek/satislar.csv --baslangic 7 --adet 3
```

```text
       # | siparis_no |         urun | adet |  fiyat |      tarih |    sehir |   not
---------+------------+--------------+------+--------+------------+--------+------
       7 |       1008 |       Defter |    1 | 120.00 | 2024-03-15 |  İzmir |
       8 |       1009 |        Kalem |    7 |  45.50 | 2024-04-02 |  Bursa |
       9 |       1010 | Kitap, novel |    4 | 199.00 | 2024-04-11 | Ankara | satış
3 / 10 satir  |  pencere 7..10  |  indeks seyrek  |  okunan 3054 bayt
```

### 4. `filter` — bir veya birden çok koşul

```bash
datalens filter ornek/satislar.csv --filtre "fiyat>200" --filtre "sehir contains an" --adet 5
```

```text
       # | siparis_no |  urun | adet |  fiyat |      tarih |    sehir |                      not
---------+------------+-------+------+--------+------------+----------+-------------------------
       6 |       1007 | Çanta |    2 | 899.90 | 2024-03-11 | İstanbul | müşteri değişti, telefo…
1 / 10 satir eslesti (%10.00)  |  2 kosul
```

### 5. `schema` — tip çıkarımı ve sütun profili (JSON)

```bash
datalens schema ornek/satislar.csv --oznek 10
```

Çıktının özeti (tam çıktı çok uzundur; `serde_json::to_string_pretty` üretir):

| sütun | tip | boş oranı | min | max | ortalama |
|---|---|---|---|---|---|
| `siparis_no` | `tam_sayi` | 0.0 | 1001 | 1010 | 1005.5 |
| `urun` | `metin` | 0.0 | — | — | — |
| `adet` | `tam_sayi` | 0.0 | 1 | 40 | 10.3 |
| `fiyat` | `kayan_nokta` | 0.0 | 42.00 | 899.90 | 253.73 |
| `tarih` | `tarih` | 0.0 | `2024-01-15` | `2024-04-11` | — |
| `sehir` | `metin` | 0.0 | — | — | — |
| `not` | `metin` | 0.6 | — | — | — |

`tarih` sütununda iki farklı biçim (`2024-01-15` ve `15.02.2024`) bulunuyor; ikisi de
geçerlidir ve `bicim_varyanti: 10` ile raporlanır. `tarih_tutarsiz` alanı, biçimi tarihe
benzeyip **geçersiz** olan değerleri (ör. `2024-13-45`) sayar.

### 6. `export` — filtre uygulanmış küneyi dışa aktarma

```bash
datalens export ornek/satislar.csv --filtre "sehir=Ankara" --cikti cikti/ankara.csv
```

```json
{
  "bicim": "csv",
  "hedef": "C:\\Users\\xXx\\AppData\\Local\\Temp\\opencode\\ankara.csv",
  "kaynak": ".\\ornek\\satislar.csv",
  "sutun_sayisi": 7,
  "toplam_satir": 10,
  "yazilan_satir": 4
}
```

JSONL hedefi için biçim uzantıdan çözülür; ayrıca `--cikti-bicim csv|jsonl` ile
zorlanabilir:

```bash
datalens export ornek/olcumler.jsonl --cikti cikti/olcum.jsonl
```

```json
{
  "bicim": "jsonl",
  "hedef": "C:\\Users\\xXx\\AppData\\Local\\Temp\\opencode\\olcum.jsonl",
  "kaynak": ".\\ornek\\olcumler.jsonl",
  "sutun_sayisi": 5,
  "toplam_satir": 6,
  "yazilan_satir": 6
}
```

### 7. TSV ve bozuk satırlar

```bash
datalens head ornek/envanter.tsv --adet 3
```

```text
       # |   kod |     ad | stok |      giris
---------+-------+--------+------+-----------
       0 | A-100 |  Kalem |  120 | 2024-01-02
       1 | A-200 | Defter |   34 | 2024-01-05
       2 | A-300 |  Çanta |    7 | 2024-01-09
3 / 5 satir  |  pencere 0..3  |  indeks seyrek  |  okunan 1073 bayt
```

`ornek/olcumler.jsonl` dosyasında bilerek bozuk bir satır vardır. DataLens onu
**düşürmez**, işaretler:

```bash
datalens sample ornek/olcumler.jsonl --baslangic 3 --adet 2 --sutun kimlik --sutun servis
```

```text
       # | kimlik | servis | !
---------+--------+--------+-
       3 |      4 |  rapor |
       4 |        |        | !
uyarilar:
  #4 gecersiz JSON: expected value at line 1 column 1
2 / 6 satir  |  pencere 3..5  |  indeks seyrek  |  okunan 6281 bayt
```

---

## Test

```bash
cargo test
```

Gerçek çıktı:

```text
   Compiling datalens v0.1.0 (%USERPROFILE%\Desktop\Projeler\projects\26-datalens)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.32s
     Running unittests src\lib.rs (target\debug\deps\datalens-ccba5513c3e51b66.exe)
     Running unittests src\main.rs (target\debug\deps\datalens-044c85921b7fe4dd.exe)
     Running tests\cli_entegrasyon.rs (target\debug\deps\cli_entegrasyon-94b576162700810a.exe)
     Running tests\entegrasyon.rs (target\debug\deps\entegrasyon-f987a4eb834a4e68.exe)
     Running tests\sabit_bellek.rs (target\debug\deps\sabit_bellek-1cf3bfabbc5f51ba.exe)
   Doc-tests datalens

test result: ok. 183 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.91s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

test sonucu: okunan 219; gecen 219; basarisiz 0
```

**Test sayıları:** 219 (birim 183 + CLI 19 + entegrasyon 13 + sabit bellek 4). Eşik 30,
aşılıyor.

### Kapsanan kenar durumları

| Alan | Test edilen durumlar |
|---|---|
| CSV ayrıştırma | tırnaklı alan, `""` kaçışı, alan içi CRLF, tırnak içi ayraç, tırnak kapatılmamış alan, tırnak sonrası veri, tırnaksız alanda tırnak, boş alanlar, boş satırlar, yalnız başlık, boş dosya, son satırsız dosya, eksik sütun, fazla sütun, geçersiz UTF-8, Unicode/emoji |
| Satır sonu | LF, CRLF, yalnız CR |
| Ayraç | virgül, noktalı virgül, sekme, boru, tırnak içi ayraç sayılmaz, eşitlikte aday sırası |
| JSONL | geçerli nesne, bozuk satır, dizi kökü, iç içe dizi, iç içe nesne, dizge içi `\n`, CRLF |
| Tip çıkarımı | tam sayı → kayan nokta → metin yükseltmesi, mantıksal, tarih, tamamen boş sütun, geçersiz tarih (`2024-13-45`), saat/son ek varyantları |
| Profil | boş oranı, min/max/ortalama, en sık değerler, ayrık değer sayacı, biçim varyantı, uyarılı satır sayacı |
| Filtre | `=`, `>`, `<`, `contains`; sayısal ve sözel karşılaştırma; büyük/küçük harf duyarsız sütun adı; sıra numarasıyla sütun seçimi; tırnaklı değer; boş hücre sınırları; çok koşul; bilinmeyen sütun/operatör hatası |
| Dışa aktarım | tırnak kaçışı, ayraç/satır sonu içeren alanlar, atomik geçici dosya temizliği, gidiş-dönüş değer koruması, JSONL tip koruması |
| Görüntüleme | kırpma, genişlik bütçesi, eksik sütun (`?`), uyarı sütunu, kontrol karakterleri |
| İndeks | iki kip aynı aralığı verir, artan/kapsayıcı aralıklar, blok sınırında atlanan kayıt yok, seyrek indeks bellek oranı, indeks tamponu büyümez, tampon uzun kayıt |
| Hata mesajları | her `Hata` türünün `Display` çıktısı ve `source` zinciri |

### Sabit bellek

Sabit bellek iddiası iki bağımsız testle ölçülür
(`tests/sabit_bellek.rs`):

1. **Sayacla ölçüm.** 1 MiB ve 64 MiB'lık aynı yapıda iki dosya açılır, her birinden son
   satır okunur. Veri geçişinde okunan bayt sayısı dosya boyutundan bağımsız olmalıdır
   (her iki indeks kipi için).
2. **Kaynak denetimi.** `src/**/*.rs` dosyalarının **üretim kodu** bölümünde
   `read_to_string`, `read_to_end`, `fs::read(` ve `read_to_string_lossy` çağrısı
   bulunmadığı doğrulanır. `#[cfg(test)]` modülleri denetim dışıdır (testlerin küçük
   dosyaları okuması sorun değildir).

Elle ölçüm (release ikilisi, `x86_64-pc-windows-gnu`, 3 çalıştırmanın ortalaması):

| Dosya | Satır | Kip | Tepe çalışma seti | Tepe özel bellek | İndeks belleği | Süre |
|---|---|---|---|---|---|---|
| 16 MB | 410.040 | seyrek | 4,3 MB | 1,0 MB | 6.408 bayt | 31 ms |
| 16 MB | 410.040 | yogun | 13,3 MB | 10,1 MB | 6.560.656 bayt | — |
| 64 MB | 1.555.507 | seyrek | 4,3 MB | 1,1 MB | 24.312 bayt | 81 ms |
| 256 MB | 5.932.172 | seyrek | **5,0 MB** | **1,3 MB** | 92.696 bayt | 286 ms |
| 256 MB | 5.932.172 | yogun | 133,3 MB | 130,2 MB | 94.914.768 bayt | 322 ms |
| 256 MB | 5.932.172 | seyrek, **son satır** | 4,5 MB | 1,3 MB | — | 285 ms |
| 256 MB | 5.932.172 | yogun, **son satır** | 133,2 MB | 130,2 MB | — | 325 ms |

Son satır penceresinde **veri geçişinde okunan bayt**: seyrek kipte 39.300 bayt, yoğun
kipte 16.430 bayt. Bu sayı dosya boyutundan bağımsızdır — 256 MB'lık dosyanın sonundan
tek satır okumak 16 KB'tır.

Rapordaki 320 MB bütçesi **seyrek kipte fazlasıyla korunur** (5,0 MB). Yoğun kip 256 MB
dosyada 133 MB ile bütçe içindedir ancak 1 GB'lık bir dosyada 380 MB'a çıkardı.

---

## Proje Yapısı

```text
26-datalens/
├── Cargo.toml
├── Cargo.lock              (üretilir, commit edilir)
├── LICENSE.txt             MIT, "Copyright (c) 2026"
├── README.md
├── .gitignore
├── ornek/                  README'deki komutların çalıştığı küçük örnek veri
│   ├── satislar.csv
│   ├── olcumler.jsonl
│   └── envanter.tsv
├── src/
│   ├── lib.rs              çekirdek kütüphane, modül listesi, lint'ler
│   ├── main.rs             CLI kabuğu (clap), çıktı biçimlendirme
│   ├── hata.rs             Hata enum'u + elle Display/Error uygulamaları
│   ├── bicim.rs            biçim (CSV/TSV/JSONL) ve ayraç algılama
│   ├── ayristirici.rs      kayıt sınırlayıcı + alan ayrıştırıcı
│   ├── indeks.rs           bayt aralıklı satır indeksi + kaydırmalı okuyucu
│   ├── kaynak.rs           satır erişim servisi, sütun çözümleme, okuma sayacları
│   ├── tip.rs              sütun tipi çıkarımı, tarih normalleştirme
│   ├── profil.rs           sütun profili ve şema çıkarımı
│   ├── filtre.rs           filtre dili, operatörler, sayısal/sözel karşılaştırma
│   ├── disa.rs             CSV ve JSONL yazıcıları, atomik dışa aktarım
│   ├── gorunum.rs          terminal tablo çizimi, genişlik bütçesi
│   └── test_yardimcisi.rs  yalnızca birim testlerde kullanılan geçici dizin
└── tests/
    ├── yardimci/mod.rs     entegrasyon testleri için geçici dizin yardımcısı
    ├── entegrasyon.rs      uçtan uca kütüphane testleri
    ├── cli_entegrasyon.rs  gerçek ikilinin çalıştırıldığı CLI testleri
    └── sabit_bellek.rs     sabit bellek kanıtı + kaynak denetimi
```

Bağımlılık yönü tek yönlüdür: `hata ← bicim ← ayristirici ← indeks ← kaynak` ve
`kaynak → {profil, filtre, gorunum, disa}`. Arayüz katmanı (`gorunum`, `main`) dosya
biçimi ayrıntılarını bilmez.

Bağımlılıklar (`Cargo.toml`, WORKER_CONTRACT.md § 3.1 gereği gerekçelendirilmiştir):

| Crate | Sürüm | Gerekçe |
|---|---|---|
| `serde` | 1 (derive) | `profil.rs` ve `hata.rs` çıktı yapı türeticileri |
| `serde_json` | 1 | JSONL kayıt doğrulama ve JSON çıktısı |
| `clap` | 4 (derive) | alt komut ve seçenek çözümlemesi |

---

## Yapılandırma

Yapılandırma **dosyası yoktur**; her şey komut satırındandır. Bu, "USB'den çalışma,
program dizinine yazma" gereksiniminin en sade karşılığıdır.

### Tüm alt komutların paylaştığı seçenekler

| Seçenek | Varsayılan | Etkisi |
|---|---|---|
| `--bicim <virgullu-ayracli\|sekmeyle-ayracli\|json-lines>` | uzantı, yoksa içerik | Dosya biçimini elle seçer. Uzantı tanınmıyorsa zorunludur (`--bicim csv`). |
| `--ayrac <, ; \t \|>` | ilk kayıttan algılanır | CSV ayracı. Tırnak içindeki ayraçlar sayılmaz. |
| `--baslik <var\|yok>` | CSV/TSV için `var`, JSONL için `yok` | `yok` verilirse ilk satır da veri sayılır, sütun adları `sutun_1..n` olur. |
| `--indeks <seyrek\|yogun>` | `seyrek` | Satır indeksi kipi. Bkz. [Özellikler](#özellikler). |
| `--blok <N>` | `512` | Seyrek kipte bir bloktaki satır sayısı. Küçültmek belleği değil, blok içi tarama maliyetini değiştirir. |
| `--sutun <AD>` | tümü | Yalnızca verilen sütunları gösterir (birden fazla verilebilir). |
| `--genislik <N>` | `200` | Terminal çıktı genişliği. Sütunlar en genişten başlanarak daraltılır. |
| `--sutun-genislik <N>` | `24` | Sütun başına azami genişlik. Hücreler `…` ile kırpılır (görüntüleme kırpması, veri kaybı değildir). |

### Alt komuta özel seçenekler

| Komut | Seçenek | Varsayılan | Etkisi |
|---|---|---|---|
| `schema` | `--oznek <N>` | `1000` | Profil için kaç satır inceleneceği. Sonuçta `ornek_satir_sayisi` ile raporlanır. |
| `schema` | `--cikti <YOL>` | stdout | Profil JSON'unu dosyaya yazar. |
| `count` | `--cikti <YOL>` | stdout | Özet JSON'unu dosyaya yazar. |
| `head` | `-n, --adet <N>` | `20` | Gösterilecek satır sayısı. |
| `sample` | `--baslangic <N>` | `0` | Pencere başlangıcı (sıfırdan). |
| `sample` | `-n, --adet <N>` | `20` | Pencere uzunluğu; dosya sonunda kırpılır. |
| `filter` | `--filtre <IFADE>` | — | Tekrarlanabilir. Her koşul `<sütun> <operatör> <değer>`. |
| `filter` | `--adet <N>` | `20` | Eşleşen satırlardan gösterilecek azami sayı. |
| `export` | `--filtre <IFADE>` | — | Tekrarlanabilir; boş bırakılırsa tüm satırlar yazılır. |
| `export` | `--cikti <YOL>` | zorunlu | Hedef dosya. Atomik yazılır. |
| `export` | `--cikti-bicim <csv\|jsonl>` | uzantıdan | Çıktı biçimi. |

### Filtre dili

```text
<sütun> <operatör> <değer>
```

| Biçim | Örnek | Açıklama |
|---|---|---|
| Sözel | `yas > 30` | Operatör `=`, `>`, `<`, `contains` (takma adlar: `icerir`, `~`, `like`; `==`, `eq`, `gt`, `lt`) |
| İnfix | `sira>5`, `ad=ali` | Boşluksuz; sütun adı iki noktalı virgül içeriyorsa kullanılamaz. |
| Tırnaklı değer | `not = "ilk okul"` | Değerin çevre tırnakları soyulur. |

Sütun adı tam eşleşme, büyük/küçük harf duyarsız eşleşme, sonra sıra numarası olarak
çözülür. Birden çok `--filtre` verilirse hepsi birlikte uygulanır (VE).

Karşılaştırma kuralı: hücre ve filtre değeri **ikisi de** sayıya çevrilebiliyorsa sayısal
karşılaştırma yapılır, aksi hâlde bayt sırasıyla sözel karşılaştırma yapılır. `contains`
her zaman büyük/küçük harf duyarsızdır.

### Ortam değişkeni, kayıt defteri, ağ

Kullanılmaz. Program hiçbir ağ bağlantısı açmaz, kayıt defterine yazmaz, ortam
değişkeni okumaz. Tek yazma yolu kullanıcının `--cikti` ile verdiği dosyadır.

---

## Bilinen Sınırlamalar

Bu bölüm dürüst olmak zorundadır. Aşağıdakilerin **hiçbiri ölçülmüş bir vaat değildir**;
raporun sayısal iddiaları bu projede doğrulanmamıştır.

### Rapordan sapmalar (MANIFEST.md Kart 26)

- **Grafik arayüz yok.** Rapor `egui`/`eframe` ile etkileşimli ızgara öngörür.
  `WORKER_CONTRACT.md` § 3.2-G pencere katmanını kalıcı olarak yasaklar; kabul edilen
  çıktı terminaldir. Fare ile kaydırma, sütun sürükle-bırak ve kalıcı sütun genişliği
  **ertelenmiştir**. "Sanal kaydırma" burada bir *pencere komutudur* (`sample`), etkileşimli
  değildir.
- **`duckdb-wasm` yok.** Rapordaki "v1: gömülü SQL sorgu modu" **tamamen düşmüştür**.
  Filtre dili kasıtlı olarak dardır (dört operatör, yalnızca VE); `SELECT ... WHERE`
  yazılamaz.
- **`arrow-rs` ve Parquet yok.** Dışa aktarım yalnızca CSV ve JSONL'dir.
- **`memmap2` yok.** `std::fs::File` + `seek` + `read` kullanılır. Bu seçim FFI
  gerektirmez ve aynı işi yapar; bedeli `seek` tabanlı okumanın `mmap`'e göre yavaş
  olmasıdır (ölçüm: 256 MB dosyanın tam indeksini kurmak 286 ms).
- **`rayon` yok.** Tek dosya için ardışık iki geçiş yeterlidir; iş parçacığı havuzu
  kurulmadı. Raporın "okuma havuzu" fikri ertelenmiştir.

### Ölçülmüş ve doğrulanmış davranışlar

- Sabit bellek: [testte ölçüldü](#sabit-bellek). 256 MB / 5,9 M satır → 5,0 MB tepe çalışma
  seti (seyrek kip).
- Son satır okuması: 16.430–39.300 bayt, dosya boyutundan bağımsız.
- 320 MB rapor bütçesi: seyrek kipte fazlasıyla korunur.

### Ölçülmemiş veya kısıtlı olanlar

- **MSRV doğrulanmadı.** `rust-version = "1.74"` bildirilmiştir ancak yalnızca 1.98.1 ile
  derlendi. 1.74'lük bir araç zinciriyle test edilmedi.
- **Bütçe 2 GB sanal makine testi yapılmadı.** Raporun b08 kabul testi (2 GB RAM'li VM'de
  S1/S2 senaryoları) bu ortamda çalıştırılmadı.
- **İlk boyama süresi ölçülmedi.** Grafik arayüz olmadığı için "ilk boyama" kavramı yoktur;
  `head` çıktısının gecikmesi ölçülmedi.
- **Diskte indeks önbelleği yok.** Rapor 18 MB'lık bir blok önbelleği öngörür. Bu MVP'de
  indeks **her çalıştırmada yeniden kurulur** (286 ms / 256 MB); diske yazılmaz, dolayısıyla
  geçersiz kalan bir önbellekten söz etmek de yoktur.
- **İş parçacığı kullanılmadığı için** çok çekirdekli makinede indeksleme tek çekirdektedir.

### Biçim ve ayrıştırma sınırları

- **Sıkıştırılmış dosyalar (`.gz`, `.zip`) desteklenmez.** Bunlar için `flate2`/`miniz_oxide`
  kart kapsamı dışında bırakılmıştır.
- **Katkod sayfası (UTF-16/UTF-32) desteklenmez.** Dosya geçerli UTF-8 olmalıdır. Geçersiz
  UTF-8 alanı **boşaltılır** ve uyarı üretilir; baytlar sessizce değiştirilmez.
- **Kapatılmamış tırnak kuralı yoktur** (RFC 4180 da tanımlamaz). Böyle bir kayıt dosya
  sonuna kadar tek kayıt sayılır ve uyarı üretilir.
- **Tırnak kapatıldıktan sonra gelen veri** (ör. `"a"b`) aynı alana eklenir ve uyarı
  üretilir. Katı biçimde reddedilmez.
- **Geniş tablo terminalde okunmaz.** Sütunlar `…` ile kırpılır; yatay kaydırma yoktur.
  Geniş tablolar için `--sutun` ile sütun seçin veya `export` kullanın.
- **Doğu Asya geniş karakter ölçümü yapılmaz.** Hücre genişliği `chars().count()` ile
  ölçülür; CJK veya emoji içeren hücrelerde sütunlar hizalı görünmeyebilir.
- **Ayrık değer sayısı sınırlıdır.** Bir sütunda 4096'dan fazla ayrık değer varsa
  `farkli_deger_sinirda: true` işaretlenir ve `farkli_deger_sayisi` tam değildir. En sık
  değerler yalnızca bu pencerenin içinde hesaplanır.
- **Ortalama `f64` ile biriktirilir.** Çok büyük tam sayı sütunlarında son ondalıkta
  yuvarlama farkı olabilir.
- **Tarih desteği dardır:** `YYYY-AA-GG`, `GG.AA.YYYY`, `GG-AA-YYYY` ve bunların saat
  kısmı (`HH:MM[:SS[.mmm]]`, `Z`, `±HH:MM`). `GG/AA/YYYY` yalnızca `YYYY` ile başlıyorsa
  tanınır; saat dilimi karşılaştırması sözel olduğu için farklı saat dilimleri aynı
  anahtarda sıralanmayabilir.
- **Mantıksal tip yalnızca `true`/`false`** değerlerini kabul eder. `1`/`0` tam sayı olarak
  sınıflandırılır; bir sütunda hem `true` hem `1` varsa sütun `tam_sayi` olur.
- **Boş satırlar atlanır** ve `atlanan_bos_satir` sayacıyla raporlanır. Bu bir veri
  kaybı değildir, ancak kaynak dosyadaki boş satır sayısıyla eşleşmez.
- **Tamamen boş bir dosyada sütun yoktur** ve `head` `(bu tabloda gosterilecek sutun yok)`
  yazar.
- **Başlık ile veri satırı sayısı karışabilir.** `count` ve `schema` **başlık hariç**
  veri satırı sayısını bildirir. Karışıklık olmaması için `ornek/satislar.csv`
  başlıksız değildir; `--baslik yok` ile açıkça belirtmelisiniz.
- **Dışa aktarım tüm eşleşen satırları belleğe toplar** (`export`). Çok büyük sonuç
  kümelerinde bellek eşleşen satır sayısıyla artar. Bu, kartın "her işlem bellek bütçesini
  aşmaz" kabul kriteriyle çelişir ve bilinçli bir MVP sınırıdır.
- **Filtre uygulaması tüm dosyayı tarar** (indeks zaten taranmıştır; sonra satırlar
  `seek` ile okunur). 5,9 M satırlık dosyada `filter` süresi indeksleme süresine yakındır.
- **Sütun adları kalıcı değildir.** Yeniden adlandırma ve şema kaydetme ertelenmiştir.
- **Tek dosya dağıtımında** Windows SmartScreen / antivirüs uyarısı çıkabilir; bu bilinen
  bir yan etkidir ve sürüm imzasıyla azaltılır.

### Politika notları

- `#![forbid(unsafe_code)]` — kendi kaynak kodumuzda `unsafe` **yoktur**; `forbid`
  gevşetilmemiştir.
- `#[allow(clippy::unwrap_used, clippy::expect_used)]` yalnızca `#[cfg(test)]` modüllerinde
  vardır (testlerde `expect` kullanmak sözleşmeye uygundur). Üretim kodunda `unwrap`,
  `expect` ve `panic!` **yoktur**.
- `forbid` → `deny` dönüşümü **yapılmadı**.
- `Drop` içindeki geçici dizin temizliğinde hatalar `let _ =` ile yutulur. `Drop`'tan
  hata döndürülemez; bu, sözleşmenin "sessiz yutma" yasağına yegdir olan tek yerdir ve
  burada da belgelenmiştir.
- Bu README'deki her komut gerçekten çalıştırılmıştır. Tablolardaki ölçümler aynı
  makinede `target\release\datalens.exe` ile alınmıştır.

---

## Gelecek Geliştirmeler

Kartın "Ertelenen" listesi ve doğal sonraki adımlar:

1. **Filtre ifadeleri** — mantıksal birleşim (`OR`), `IN`, `LIKE`, aralık (`BETWEEN`).
   Kapalı gramer korunmalı; dosya/ağ/süreç işlevi eklenmemeli.
2. **Ardışık taramada akış modu** — indeks atlanıp yalnızca filtre sonucu yazılabilir
   (`--akis`). Bu, `export`ın bellek sınırını kaldırır.
3. **Diske indeks önbelleği** — dosya boyutu + son değişiklik zamanı + biçim ile anahtarlanan
   `cache/` dosyası. Raporun 18 MB hedefi. Geçersiz kalmama `mtime` + boyut imzasıyla
   denetlenmelidir.
4. **Parquet / Arrow dışa aktarımı** — `arrow-rs` ve `parquet` crate'leri kart kapsamı
   dışında bırakılmıştır; karar yeni bir maddeyle verilmelidir.
5. **Sıkıştırılmış girdi** — `.gz` için `flate2` (`miniz_oxide`).
6. **Geniş tablolar için yatay kaydırma** — terminal genişliği yerine "sütun grubu"
   seçimi (`--sutun-grup`).
7. **Profil kapsamı** — varsayılan 1000 satır yerine `--oznek` ile tüm dosya
   taranabilsin (ileri/geri örnekleme).
8. **Sütun adı normalleştirme ve kalıcı eşleme** — rapordaki "kolon yeniden adlandırma
   kalıcılığı".
9. **Gömülü SQL modu** — DuckDB-WASM yerine saf Rust'ta bir SQL alt kümesi; bu, kartın
   kabul ettiği riskli bir kapsam genişletmesidir ve ayrı bir karar gerektirir.

---

## Troubleshooting

### 1. `dosya bicimi belirlenemedi: <yol> (uzanti ve icerik yetersiz; --bicim ile belirtin)`

**Belirti:** `datalens count veri.dat` hata verir, dosya CSV olmasına rağmen.

**Neden:** Uzantısı bilinmeyen (`.dat`, `.txt` dışı) ve ilk anlamlı baytı `{` ya da `[`
olmayan dosyalarda içerik sezgisi çalışmaz.

**Çözüm:** Biçimi elle verin.

```bash
datalens count veri.dat --bicim virgullu-ayracli
```

### 2. `filtre ifadesi cozulemedi: "yas" (sutun adi eksik)`

**Belirti:** `--filtre "yas"` yazdığınızda hata alırsınız.

**Neden:** Filtre dilinde sütun adı, operatör ve değer olmak üzere üç parça gerekir.
Yalnız sütun adı yazmak eksik bir ifadedir.

**Çözüm:** Operatörü ve değeri ekleyin.

```bash
datalens filter veri.csv --filtre "yas>30"
```

### 3. `gecersiz satir araligi: 99..100 (dosyada 10 satir var)`

**Belirti:** `sample --baslangic 99` ile dosyanın sonundan ötesini istediğinizde hata alırsınız.

**Neden:** Sanal kaydırma penceresi `0 .. satir_sayisi` aralığına sınırlıdır. Satır
numaraları sıfırdan başlar ve **başlık satırı sayılmaz**.

**Çözüm:** Son satırları almak için `count` ile satır sayısını öğrenin, sonra
`--baslangic satir_sayisi - N` kullanın.

```bash
datalens count veri.csv
datalens sample veri.csv --baslangic 8 --adet 2
```

### 4. Tabloda bir sütun `?` görünüyor

**Belirti:** Son sütunda `?` ve uyarılarda `sutun sayisi uyusmuyor` mesajı çıkar.

**Neden:** Kaynak satırda daha az alan var. Genellikle **tırnak içinde olması gereken
virgül** tırnaksız yazılmıştır.

**Çözüm:** Alanı tırnaklayın (`"Kalem, kırmızı"`) ya da veriyi düzeltin. DataLens bu
satırı düşürmez; işaretler.

### 5. `sutun bulunamadi: "yas" (tablo 3 sutun iceriyor)`

**Belirti:** Filtrede kullandığınız sütun adı hata veriyor.

**Neden:** Başlıkta Türkçe karakterli veya boşluk içeren sütun adları normalleştirilir:
boş ad `sutun_1` olur, yinelenen adlar `ad_2`, `ad_3` olur.

**Çözüm:** Gerçek adı öğrenin ve sıra numarasıyla da filtreleyebilirsiniz.

```bash
datalens count veri.csv
datalens filter veri.csv --filtre "1>30"
```

### 6. Dışa aktarımda eski dosya kalmadı / `cikti yazilamadi`

**Belirti:** `export` hedefe yazarken hata verir.

**Neden:** Hedef dizin yoksa ya da dosya kilitliyse (ör. Excel'de açık) yazma başarısız
olur. Yazım atomiktir: hedef önce **silinmez**, geçici dosyaya yazılır; hata olursa
geçici dosya silinir.

**Çözüm:** `--cikti` için var olan bir dizin verin ve hedef dosyayı kapatın. Geçici
dosya `hedef.datalens-ortaci` adıyla oluşur; hata durumunda kendiliğinden silinir.

---

## Atıflar

- **RFC 4180 — Common Format and MIME Type for Comma-Separated Values (CSV)**,
  <https://www.rfc-editor.org/rfc/rfc4180> — tırnaklı alanlar, `""` kaçışı, CRLF satır
  sonu, başlık satırı kuralları.
- **JSON Lines (jsonlines.org)** — <https://jsonlines.org/> ve
  <https://jsonlines.org/errata.html> — satır başına tek JSON değeri biçimi; bozuk satır
  davranışı.
- **Rust standart kütüphane — `std::fs::File`, `Seek`, `Read`** —
  <https://doc.rust-lang.org/std/fs/struct.File.html> — konum bazlı okuma ve
  `std::io::BufReader`'ın neden kullanılmadığı.
- **Rust standart kütüphane — `std::time` yerine kullanılan `Instant`/`SystemTime` yoktur**;
  tarih ayrıştırma tamamen elle yazılmıştır. <https://doc.rust-lang.org/std/>
- **Rust 2021 edition rehberi** — <https://doc.rust-lang.org/edition-guide/edition-2021/>
- **`serde`** — <https://serde.rs/> ve <https://docs.rs/serde/> — türetilmiş serileştirme.
- **`serde_json`** — <https://github.com/serde-rs/json> ve
  <https://docs.rs/serde_json/> — JSON ayrıştırma/serileştirme.
- **`clap`** — <https://docs.rs/clap/> — alt komut ve seçenek çözümlemesi.
- **İleriye dönük okuma referansları (kullanılmadı, yalnızca davranış referansı):**
  `csv` crate — <https://docs.rs/csv/> (raporda önerilmişti, bağımlılık politikası
  nedeniyle kullanılmadı); `qsv` — <https://github.com/harbourmaster/qsv>;
  Miller — <https://miller.readthedocs.io/>.
- **Rapor dosyası (iç tasarımın kaynağı):**
  `%USERPROFILE%\Desktop\Fikirler\26-veri-mercek-csv-json.html` — yerel dosyadır, URL
  değildir. Bu README'deki mimari kararlar (bayt aralıklı indeks, sanal kaydırma penceresi,
  sütun profili, filtre dili) bu raporun b05, b06, b07, b08 ve b09 bölümlerinden
  türetilmiştir.
- **Doğrudan kopyalanan kod yoktur.** Tüm ayrıştırma, indeksleme, filtre ve gösterim
  kodu bu proje için sıfırdan yazılmıştır.

---

## Üretim Atfı

Bu depo **OpenCode** ajanı tarafından, **`space-bunny-free`** modeli
(`opencode/space-bunny-free`) kullanılarak üretilmiştir.

- **Arac:** OpenCode
- **Model:** `opencode/space-bunny-free` (Space Bunny Free)
- **Tür:** Rust, `cargo build` / `cargo test` ile üretilmiş ve doğrulanmıştır.

Kaynak kod, testler ve dokümantasyon bu model tarafından yazılmıştır. İnsan
katkısı: gereksinim tanımı, kabul ölçütleri ve son kontroller.

## Lisans

MIT. Tam metin: [`LICENSE.txt`](LICENSE.txt).

Telif: `Copyright (c) 2026 VeriMercek (DataLens) contributors`.
