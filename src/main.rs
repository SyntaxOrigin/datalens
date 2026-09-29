//! DataLens komut satırı arayüzü.
//!
//! Bu dosya yalnızca komut satırı ayrıştırma, çıktı biçimlendirme ve hata
//! yazımı yapar; dosya okuma mantığı `datalens` kütüphanesindedir.
//!
//! Çıkış kodu: `0` başarı, `1` hata, `2` kullanım hatası (clap'in kendi kodu).

#![forbid(unsafe_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};

use datalens::bicim::Bicim;
use datalens::disa::{disa_aktar, DisaBiimi};
use datalens::filtre::FiltreKumesi;
use datalens::gorunum::{tablo_yaz, uyarilari_yaz, GorunumAyar};
use datalens::hata::Hata;
use datalens::indeks::{IndeksKipi, Sablon};
use datalens::kaynak::{Ayarlar, Kaynak, Satir};
use datalens::profil::profil_uret;

/// VeriMercek — büyük CSV/TSV/JSONL dosyalarını sabit bellekle açan görüntüleyici.
#[derive(Debug, Parser)]
#[command(
    name = "datalens",
    version,
    about = "CSV, TSV ve JSONL dosyalarini sabit bellekle acar, profiller ve disa aktarir.",
    long_about = "VeriMercek (DataLens), dosyayi iki geciste okur: once satirlarin bayt \
                  araliklari indekslenir, sonra yalnizca istenen satirlar konum bazli okunur. \
                  Dosyanin tamami hicbir zaman bellege alinmaz."
)]
struct Cli {
    #[command(subcommand)]
    komut: Komut,
}

/// Biçim seçeneğinin karşılığı.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BicimArg {
    VirgulluAyracli,
    SekmeyleAyracli,
    JsonLines,
}

impl BicimArg {
    fn bicim(self) -> Bicim {
        match self {
            BicimArg::VirgulluAyracli => Bicim::Csv,
            BicimArg::SekmeyleAyracli => Bicim::Tsv,
            BicimArg::JsonLines => Bicim::Jsonl,
        }
    }
}

/// İndeks kipi seçeneğinin karşılığı.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum IndeksArg {
    /// Her satır için 16 bayt: hızlı, bellek satır sayısıyla artar.
    Yogun,
    /// Her 512 satır için 8 bayt: bellek dosya boyutundan bağımsızdır.
    Seyrek,
}

impl IndeksArg {
    fn kip(self) -> IndeksKipi {
        match self {
            IndeksArg::Yogun => IndeksKipi::Yogun,
            IndeksArg::Seyrek => IndeksKipi::Seyrek,
        }
    }
}

/// Dışa aktarım biçimi seçeneğinin karşılığı.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DisaArg {
    Csv,
    Jsonl,
}

/// Alt komutlar.
#[derive(Debug, Subcommand)]
enum Komut {
    /// Sütun profili ve şemayı üretir (varsayılan çıktı JSON).
    Schema {
        /// İncelenecek dosya.
        dosya: PathBuf,
        #[command(flatten)]
        ortak: OrtakSecenekler,
        /// Örneklem için bakılacak satır sayısı (varsayılan 1000).
        #[arg(long, default_value_t = 1000, value_name = "N")]
        oznek: u64,
        /// Çıktıyı terminal yerine bu dosyaya yazar.
        #[arg(long, value_name = "YOL")]
        cikti: Option<PathBuf>,
    },
    /// Satır ve sütun sayısı ile kısa bir özet yazar.
    Count {
        /// İncelenecek dosya.
        dosya: PathBuf,
        #[command(flatten)]
        ortak: OrtakSecenekler,
        /// Çıktıyı terminal yerine bu dosyaya yazar.
        #[arg(long, value_name = "YOL")]
        cikti: Option<PathBuf>,
    },
    /// Dosyanın başındaki N satırı tablo olarak basar.
    Head {
        /// İncelenecek dosya.
        dosya: PathBuf,
        #[command(flatten)]
        ortak: OrtakSecenekler,
        /// Gösterilecek satır sayısı (varsayılan 20).
        #[arg(long, short, default_value_t = 20, value_name = "N")]
        adet: u64,
    },
    /// Sanal kaydırma penceresi: belirli satır aralığını basar.
    Sample {
        /// İncelenecek dosya.
        dosya: PathBuf,
        #[command(flatten)]
        ortak: OrtakSecenekler,
        /// Başlangıç satırı (sıfırdan başlar).
        #[arg(long, default_value_t = 0, value_name = "N")]
        baslangic: u64,
        /// Gösterilecek satır sayısı (varsayılan 20).
        #[arg(long, short, default_value_t = 20, value_name = "N")]
        adet: u64,
    },
    /// Filtreyi uygular; eşleşen satır sayısını ve isteğe bağlı satırları yazar.
    Filter {
        /// İncelenecek dosya.
        dosya: PathBuf,
        #[command(flatten)]
        ortak: OrtakSecenekler,
        /// Filtre ifadesi: `<sütun> <operatör> <değer>`. Birden fazla verilebilir,
        /// hepsi birlikte (VE) uygulanır. Operatörler: =, >, <, contains.
        #[arg(long = "filtre", value_name = "IFADE")]
        filtreler: Vec<String>,
        /// Eşleşen satırlardan gösterilecek azami sayı (varsayılan 20).
        #[arg(long, default_value_t = 20, value_name = "N")]
        adet: u64,
    },
    /// Filtre uygulanmış küneyi CSV veya JSONL olarak disa aktarır.
    Export {
        /// İncelenecek dosya.
        dosya: PathBuf,
        #[command(flatten)]
        ortak: OrtakSecenekler,
        /// Dışa aktarılacak satırları seçen filtre ifadeleri.
        #[arg(long = "filtre", value_name = "IFADE")]
        filtreler: Vec<String>,
        /// Çıktı dosyası (atomik olarak yazılır).
        #[arg(long, value_name = "YOL")]
        cikti: PathBuf,
        /// Çıktı biçimi (varsayılan: hedef dosyanın uzantısına göre).
        #[arg(long = "cikti-bicim", value_name = "BIÇIM")]
        cikti_bicim: Option<DisaArg>,
    },
}

/// Tüm alt komutların paylaştığı seçenekler.
#[derive(Debug, Args, Clone)]
struct OrtakSecenekler {
    /// Dosya biçimini elle seç (belirlenemezse zorunludur).
    #[arg(long, value_name = "BIÇIM")]
    bicim: Option<BicimArg>,
    /// CSV ayracı: virgül, noktalı virgül, sekme veya boru.
    #[arg(long, value_name = "AYRAC")]
    ayrac: Option<String>,
    /// Başlık satırı var mı? (yok yazılırsa ilk satır da veri sayılır).
    #[arg(long, value_name = "DURUM")]
    baslik: Option<String>,
    /// Satır indeksi kipi: yogun (hızlı) veya seyrek (sabit bellek).
    #[arg(long, value_name = "KIP", default_value = "seyrek")]
    indeks: IndeksArg,
    /// Seyrek kipte bir bloktaki satır sayısı (varsayılan 512).
    #[arg(long, default_value_t = 512, value_name = "N")]
    blok: usize,
    /// Yalnızca bu sütunları göster (birden fazla verilebilir).
    #[arg(long = "sutun", value_name = "AD")]
    sutunlar: Vec<String>,
    /// Terminal çıktı genişliği (varsayılan 200).
    #[arg(long, default_value_t = 200, value_name = "N")]
    genislik: usize,
    /// Sütun başına azami genişlik (varsayılan 24).
    #[arg(long = "sutun-genislik", default_value_t = 24, value_name = "N")]
    sutun_genislik: usize,
}

impl OrtakSecenekler {
    /// Ortak seçenekleri kütüphane ayarlarına çevirir.
    fn ayarlar(&self) -> Result<Ayarlar, Hata> {
        let ayrac = match &self.ayrac {
            None => None,
            Some(metin) => Some(ayrac_bayt(metin)?),
        };
        let baslik = match self.baslik.as_deref() {
            None => None,
            Some("var") | Some("VAR") | Some("true") | Some("evet") => Some(true),
            Some("yok") | Some("YOK") | Some("false") | Some("hayir") => Some(false),
            Some(diger) => {
                return Err(Hata::BilinmeyenBicim {
                    yol: PathBuf::from(format!("--baslik {diger:?}")),
                })
            }
        };
        Ok(Ayarlar {
            bicim: self.bicim.map(BicimArg::bicim),
            ayrac,
            baslik,
            indeks: self.indeks.kip(),
            sablon: Sablon {
                blok_satir: self.blok.max(1),
                ..Sablon::default()
            },
        })
    }

    /// Görünüm ayarlarını üretir.
    fn gorunum(&self) -> GorunumAyar {
        GorunumAyar {
            genislik: self.genislik.max(20),
            sutun_genislik: self.sutun_genislik.max(3),
        }
    }

    /// Görüntülenecek sütun sıralarını çözer.
    fn sutun_sirasi(&self, kaynak: &Kaynak) -> Result<Vec<usize>, Hata> {
        let mut sonuc = Vec::with_capacity(self.sutunlar.len());
        for ad in &self.sutunlar {
            match kaynak.sutun_sira(ad) {
                Some(i) => sonuc.push(i),
                None => {
                    return Err(Hata::SutunYok {
                        sutun: ad.clone(),
                        sutun_sayisi: kaynak.basliklar().len(),
                    })
                }
            }
        }
        Ok(sonuc)
    }
}

/// Metinsel ayraç argümanını tek bayta çevirir.
fn ayrac_bayt(metin: &str) -> Result<u8, Hata> {
    let bayt = match metin {
        "," => b',',
        ";" => b';',
        "\\t" | "tab" | "sekme" => b'\t',
        "|" => b'|',
        diger if diger.chars().count() == 1 => {
            let ilk = diger.chars().next().unwrap_or('\0');
            let mut baytlar = [0u8; 4];
            if ilk.encode_utf8(&mut baytlar).len() == 1 {
                baytlar[0]
            } else {
                return Err(Hata::GecersizAyrac {
                    deger: diger.to_string(),
                });
            }
        }
        diger => {
            return Err(Hata::GecersizAyrac {
                deger: diger.to_string(),
            })
        }
    };
    Ok(bayt)
}

/// Çıktıyı ya ekrana ya da dosyaya yazar.
fn cikti_yaz(cikti: &Option<PathBuf>, metin: &str) -> Result<(), Hata> {
    match cikti {
        None => {
            let mut kilit = std::io::stdout().lock();
            writeln!(kilit, "{metin}").map_err(|e| Hata::CiktiHatasi {
                yol: PathBuf::from("<stdout>"),
                kaynak: e,
            })?;
            Ok(())
        }
        Some(yol) => {
            std::fs::write(yol, format!("{metin}\n")).map_err(|kaynak| Hata::CiktiHatasi {
                yol: yol.clone(),
                kaynak,
            })
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match calistir(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(hata) => {
            eprintln!("datalens: {hata}");
            ExitCode::FAILURE
        }
    }
}

/// Komutu yürütür.
fn calistir(cli: Cli) -> Result<(), Hata> {
    match cli.komut {
        Komut::Schema {
            dosya,
            ortak,
            oznek,
            cikti,
        } => komut_schema(&dosya, &ortak, oznek, cikti),
        Komut::Count {
            dosya,
            ortak,
            cikti,
        } => komut_count(&dosya, &ortak, cikti),
        Komut::Head { dosya, ortak, adet } => komut_pencere(&dosya, &ortak, 0, adet),
        Komut::Sample {
            dosya,
            ortak,
            baslangic,
            adet,
        } => komut_pencere(&dosya, &ortak, baslangic, adet),
        Komut::Filter {
            dosya,
            ortak,
            filtreler,
            adet,
        } => komut_filtre(&dosya, &ortak, &filtreler, adet),
        Komut::Export {
            dosya,
            ortak,
            filtreler,
            cikti,
            cikti_bicim,
        } => komut_disa(&dosya, &ortak, &filtreler, &cikti, cikti_bicim),
    }
}

/// `schema` alt komutunun gövdesi.
fn komut_schema(
    dosya: &Path,
    ortak: &OrtakSecenekler,
    oznek: u64,
    cikti: Option<PathBuf>,
) -> Result<(), Hata> {
    let ayarlar = ortak.ayarlar()?;
    let mut kaynak = Kaynak::ac(dosya, ayarlar)?;
    let adet = oznek.min(kaynak.satir_sayisi());
    let satirlar = kaynak.pencere(0, adet)?;
    let profil = profil_uret(
        kaynak.bilgi(),
        kaynak.basliklar(),
        &satirlar,
        kaynak.satir_sayisi(),
        kaynak.atlanan_bos_satir(),
    );
    let metin = serde_json::to_string_pretty(&profil).map_err(|e| Hata::CiktiHatasi {
        yol: PathBuf::from("<bellek>"),
        kaynak: std::io::Error::other(e.to_string()),
    })?;
    cikti_yaz(&cikti, &metin)
}

/// `count` alt komutunun gövdesi.
fn komut_count(dosya: &Path, ortak: &OrtakSecenekler, cikti: Option<PathBuf>) -> Result<(), Hata> {
    let ayarlar = ortak.ayarlar()?;
    let kaynak = Kaynak::ac(dosya, ayarlar)?;
    let bilgi = kaynak.bilgi();
    let istatistik = kaynak.istatistik();
    let ozet = serde_json::json!({
        "dosya": bilgi,
        "satir_sayisi": kaynak.satir_sayisi(),
        "sutun_sayisi": kaynak.basliklar().len(),
        "sutunlar": kaynak.basliklar(),
        "atlanan_bos_satir": kaynak.atlanan_bos_satir(),
        "indeks_bellek_bayt": bilgi.indeks_bellek_bayt,
        "indeks_gecisi_bayt": istatistik.indeks_bayt,
    });
    let metin = serde_json::to_string_pretty(&ozet).map_err(|e| Hata::CiktiHatasi {
        yol: PathBuf::from("<bellek>"),
        kaynak: std::io::Error::other(e.to_string()),
    })?;
    cikti_yaz(&cikti, &metin)
}

/// `head` ve `sample` alt komutlarının ortak gövdesi.
fn komut_pencere(
    dosya: &Path,
    ortak: &OrtakSecenekler,
    baslangic: u64,
    adet: u64,
) -> Result<(), Hata> {
    let ayarlar = ortak.ayarlar()?;
    let mut kaynak = Kaynak::ac(dosya, ayarlar)?;
    let sutunlar = ortak.sutun_sirasi(&kaynak)?;
    let satirlar = kaynak.pencere(baslangic, adet)?;
    let ayar = ortak.gorunum();
    let mut tampon: Vec<u8> = Vec::new();
    tablo_yaz(&mut tampon, kaynak.basliklar(), &sutunlar, &satirlar, &ayar).map_err(|e| {
        Hata::CiktiHatasi {
            yol: PathBuf::from("<stdout>"),
            kaynak: e,
        }
    })?;
    uyarilari_yaz(&mut tampon, &satirlar).map_err(|e| Hata::CiktiHatasi {
        yol: PathBuf::from("<stdout>"),
        kaynak: e,
    })?;
    let metin = String::from_utf8_lossy(&tampon).into_owned();
    let bas = if satirlar.is_empty() {
        baslangic
    } else {
        satirlar[0].no
    };
    let son = bas.saturating_add(satirlar.len() as u64);
    let alt = format!(
        "{} / {} satir  |  pencere {}..{}  |  indeks {}  |  okunan {} bayt",
        satirlar.len(),
        kaynak.satir_sayisi(),
        bas,
        son,
        kaynak.bilgi().indeks_kipi,
        kaynak.istatistik().veri_bayt
    );
    cikti_yaz(&None, &format!("{metin}{alt}"))
}

/// `filter` alt komutunun gövdesi.
fn komut_filtre(
    dosya: &Path,
    ortak: &OrtakSecenekler,
    filtreler: &[String],
    adet: u64,
) -> Result<(), Hata> {
    let ayarlar = ortak.ayarlar()?;
    let mut kaynak = Kaynak::ac(dosya, ayarlar)?;
    let basliklar = kaynak.basliklar().to_vec();
    let kume = FiltreKumesi::derle(filtreler, &basliklar)?;
    let satir_sayisi = kaynak.satir_sayisi();
    let eslesenler = kume.uygula(satir_sayisi, |no| {
        kaynak.satir(no).unwrap_or_else(|_| bos_satir(no))
    });

    let sutunlar = ortak.sutun_sirasi(&kaynak)?;
    let gosterilecek: Vec<Satir> = eslesenler
        .iter()
        .take(adet as usize)
        .filter_map(|no| kaynak.satir(*no).ok())
        .collect();
    let ayar = ortak.gorunum();
    let mut tampon: Vec<u8> = Vec::new();
    tablo_yaz(&mut tampon, &basliklar, &sutunlar, &gosterilecek, &ayar).map_err(|e| {
        Hata::CiktiHatasi {
            yol: PathBuf::from("<stdout>"),
            kaynak: e,
        }
    })?;
    uyarilari_yaz(&mut tampon, &gosterilecek).map_err(|e| Hata::CiktiHatasi {
        yol: PathBuf::from("<stdout>"),
        kaynak: e,
    })?;
    let oran = if satir_sayisi == 0 {
        0.0
    } else {
        eslesenler.len() as f64 * 100.0 / satir_sayisi as f64
    };
    let alt = format!(
        "{} / {} satir eslesti (%{oran:.2})  |  {} kosul",
        eslesenler.len(),
        satir_sayisi,
        kume.kosullar.len()
    );
    let metin = String::from_utf8_lossy(&tampon).into_owned();
    cikti_yaz(&None, &format!("{metin}{alt}"))
}

/// `export` alt komutunun gövdesi.
fn komut_disa(
    dosya: &Path,
    ortak: &OrtakSecenekler,
    filtreler: &[String],
    cikti: &PathBuf,
    cikti_bicim: Option<DisaArg>,
) -> Result<(), Hata> {
    let ayarlar = ortak.ayarlar()?;
    let mut kaynak = Kaynak::ac(dosya, ayarlar)?;
    let kume = FiltreKumesi::derle(filtreler, kaynak.basliklar())?;
    let sutunlar = ortak.sutun_sirasi(&kaynak)?;

    let secilen: Vec<u64> = if kume.bos_mu() {
        (0..kaynak.satir_sayisi()).collect()
    } else {
        kume.uygula(kaynak.satir_sayisi(), |no| {
            kaynak.satir(no).unwrap_or_else(|_| bos_satir(no))
        })
    };
    let mut satirlar: Vec<Satir> = Vec::with_capacity(secilen.len());
    for no in &secilen {
        if let Ok(satir) = kaynak.satir(*no) {
            satirlar.push(sutunlari_kisalt(&satir, &sutunlar));
        }
    }

    // Sutun secimi varsa basliklar da ayni sirayla kisaltilir.
    let tum_basliklar = kaynak.basliklar().to_vec();
    let basliklar: Vec<String> = if sutunlar.is_empty() {
        tum_basliklar
    } else {
        sutunlar
            .iter()
            .map(|i| {
                tum_basliklar
                    .get(*i)
                    .cloned()
                    .unwrap_or_else(|| format!("sutun_{}", i + 1))
            })
            .collect()
    };

    let bicim = match cikti_bicim {
        Some(b) => match b {
            DisaArg::Csv => DisaBiimi::Csv,
            DisaArg::Jsonl => DisaBiimi::Jsonl,
        },
        None => DisaBiimi::ayristir(cikti.extension().and_then(|e| e.to_str()).unwrap_or("csv"))
            .unwrap_or(DisaBiimi::Csv),
    };
    let sonuc = disa_aktar(cikti, bicim, &basliklar, &satirlar)?;
    let ozet = serde_json::json!({
        "kaynak": dosya,
        "hedef": cikti,
        "bicim": sonuc.bicim,
        "yazilan_satir": sonuc.satir,
        "sutun_sayisi": sonuc.sutun,
        "toplam_satir": kaynak.satir_sayisi(),
    });
    let metin = serde_json::to_string_pretty(&ozet).map_err(|e| Hata::CiktiHatasi {
        yol: cikti.clone(),
        kaynak: std::io::Error::other(e.to_string()),
    })?;
    cikti_yaz(&None, &metin)
}

/// Bir satırı verilen sütun sırasına göre kısaltır.
fn sutunlari_kisalt(satir: &Satir, sutunlar: &[usize]) -> Satir {
    if sutunlar.is_empty() {
        return satir.clone();
    }
    let alanlar = sutunlar
        .iter()
        .map(|i| satir.alanlar.get(*i).cloned().unwrap_or_default())
        .collect();
    Satir {
        no: satir.no,
        alanlar,
        json: None,
        uyari: satir.uyari.clone(),
    }
}

/// Okunamayan bir satır için boş doldurulmuş satır üretir.
///
/// `filter` ve `export` akışında tek bir bozuk satır tüm komutu düşürmemeli;
/// hata zaten o satırın `uyari` alanında görünür.
fn bos_satir(no: u64) -> Satir {
    Satir {
        no,
        alanlar: Vec::new(),
        json: None,
        uyari: Some("satir okunamadi".to_string()),
    }
}
