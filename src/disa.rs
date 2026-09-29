//! Dışa aktarım: CSV ve JSONL yazıcıları.
//!
//! Sorumluluğu, filtre uygulanmış satır kümesini diske yazmaktır. Yazım
//! kuralları:
//!
//! * **CSV** — RFC 4180 uyumlu. Alan ayraç, tırnak veya satır sonu içeriyorsa
//!   tırnaklanır, tırnaklar `""` ile kaçışlanır. Satır sonu `CRLF`'dir.
//! * **JSONL** — satır başına tek JSON nesnesi (jsonlines.org biçimi). JSONL
//!   kaynaktan gelen satırlar özgün JSON değerlerinden yeniden üretilir; böylece
//!   sayı/boolean/iç içe nesne tipleri metne indirgenmez.
//!
//! Raporun b10 tehdit modeli uyarınca yazma **atomiktir**: hedefin aynı
//! dizininde geçici bir adla açılır, hata olursa silinir, başarıda hedefe
//! taşınır. Böylece yarım kalmış bir dışa aktarım sır kalmaz.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::hata::Hata;
use crate::kaynak::Satir;

/// Dışa aktarım biçimleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisaBiimi {
    /// RFC 4180 uyumlu düz metin.
    Csv,
    /// Satır başına tek JSON nesnesi.
    Jsonl,
}

impl DisaBiimi {
    /// Biçimin adı.
    pub fn ad(self) -> &'static str {
        match self {
            DisaBiimi::Csv => "csv",
            DisaBiimi::Jsonl => "jsonl",
        }
    }

    /// Metinden biçim çözer (`csv`, `jsonl`, `tsv`).
    pub fn ayristir(metin: &str) -> Option<Self> {
        match metin.trim().to_ascii_lowercase().as_str() {
            "csv" => Some(DisaBiimi::Csv),
            "jsonl" | "ndjson" => Some(DisaBiimi::Jsonl),
            _ => None,
        }
    }
}

/// CSV alan ayracının metin gösterimi.
const CSV_AYRAC_METIN: &str = ",";
/// CSV satır sonu (RFC 4180 CRLF).
const CSV_SATIR_SONU: &str = "\r\n";
/// Atomik yazma için kullanılan geçici dosya son eki.
const GECICI_EK: &str = ".datalens-ortaci";

/// Bir alanı RFC 4180 kurallarına göre kaçışlar.
pub fn csv_alan_kacis(deger: &str) -> String {
    let kacis_gerekli = deger.is_empty()
        || deger.contains(CSV_AYRAC_METIN)
        || deger.contains(['"', '\n', '\r'])
        || deger.starts_with(' ')
        || deger.ends_with(' ');
    if !kacis_gerekli {
        return deger.to_string();
    }
    let mut sonuc = String::with_capacity(deger.len() + 2);
    sonuc.push('"');
    for c in deger.chars() {
        if c == '"' {
            sonuc.push('"');
        }
        sonuc.push(c);
    }
    sonuc.push('"');
    sonuc
}

/// Dışa aktarım sonucunu döndürür.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisaSonuc {
    /// Yazılan satır sayısı (başlık dahil değil).
    pub satir: u64,
    /// Yazılan sütun sayısı.
    pub sutun: usize,
    /// Biçim adı.
    pub bicim: &'static str,
}

/// Satırları hedefe atomik olarak yazar.
///
/// Geçici dosya adı `hedef.datalens-ortaci`; hata oluşursa bu dosya silinir ve
/// kullanıcıya yalnızca hata iletilir.
pub fn disa_aktar(
    hedef: &Path,
    bicim: DisaBiimi,
    basliklar: &[String],
    satirlar: &[Satir],
) -> Result<DisaSonuc, Hata> {
    match hedef.parent() {
        Some(ust) if !ust.as_os_str().is_empty() && !ust.exists() => {
            return Err(Hata::CiktiHatasi {
                yol: hedef.to_path_buf(),
                kaynak: std::io::Error::other("hedef dizini yok"),
            });
        }
        _ => {}
    }
    let gecici: PathBuf = gecici_yol(hedef);
    let sonuc = yaz_gecici(&gecici, bicim, basliklar, satirlar);
    match sonuc {
        Ok(adet) => {
            if let Err(kaynak) = fs::rename(&gecici, hedef) {
                let _ = fs::remove_file(&gecici);
                return Err(Hata::CiktiHatasi {
                    yol: hedef.to_path_buf(),
                    kaynak,
                });
            }
            Ok(DisaSonuc {
                satir: adet,
                sutun: basliklar.len(),
                bicim: bicim.ad(),
            })
        }
        Err(hata) => {
            let _ = fs::remove_file(&gecici);
            Err(hata)
        }
    }
}

/// Geçici dosya yolunu üretir.
fn gecici_yol(hedef: &Path) -> PathBuf {
    let ad = hedef
        .file_name()
        .map(|a| a.to_string_lossy().into_owned())
        .unwrap_or_else(|| "cikti".to_string());
    match hedef.parent() {
        Some(ust) if !ust.as_os_str().is_empty() => ust.join(format!("{ad}{GECICI_EK}")),
        _ => PathBuf::from(format!("{ad}{GECICI_EK}")),
    }
}

/// Geçici dosyaya yazar ve yazılan satır sayısını döndürür.
fn yaz_gecici(
    gecici: &Path,
    bicim: DisaBiimi,
    basliklar: &[String],
    satirlar: &[Satir],
) -> Result<u64, Hata> {
    let dosya = File::create(gecici).map_err(|e| Hata::CiktiHatasi {
        yol: gecici.to_path_buf(),
        kaynak: e,
    })?;
    let mut yazici = BufWriter::new(dosya);
    let hata = |kaynak: std::io::Error| Hata::CiktiHatasi {
        yol: gecici.to_path_buf(),
        kaynak,
    };

    match bicim {
        DisaBiimi::Csv => {
            if !basliklar.is_empty() {
                let baslik: Vec<String> = basliklar.iter().map(|b| csv_alan_kacis(b)).collect();
                yazici
                    .write_all(baslik.join(CSV_AYRAC_METIN).as_bytes())
                    .map_err(hata)?;
                yazici.write_all(CSV_SATIR_SONU.as_bytes()).map_err(hata)?;
            }
            for satir in satirlar {
                let alanlar: Vec<String> =
                    satir.alanlar.iter().map(|a| csv_alan_kacis(a)).collect();
                yazici
                    .write_all(alanlar.join(CSV_AYRAC_METIN).as_bytes())
                    .map_err(hata)?;
                yazici.write_all(CSV_SATIR_SONU.as_bytes()).map_err(hata)?;
            }
        }
        DisaBiimi::Jsonl => {
            for satir in satirlar {
                let deger = match &satir.json {
                    // JSONL kaynagi: ozgun deger dogrudan yazilir, boylece
                    // sayi/boolean/ic ice nesne tipleri korunur.
                    Some(v) => v.clone(),
                    // CSV/TSV kaynagi: basliklardan tek duz nesne kurulur.
                    None => {
                        let mut harita = serde_json::Map::new();
                        for (i, ad) in basliklar.iter().enumerate() {
                            let metin = satir.alanlar.get(i).cloned().unwrap_or_else(String::new);
                            harita.insert(ad.clone(), serde_json::Value::String(metin));
                        }
                        serde_json::Value::Object(harita)
                    }
                };
                let metin = serde_json::to_string(&deger).map_err(|e| Hata::CiktiHatasi {
                    yol: gecici.to_path_buf(),
                    kaynak: std::io::Error::other(e.to_string()),
                })?;
                writeln!(yazici, "{metin}").map_err(hata)?;
            }
        }
    }
    yazici.flush().map_err(hata)?;
    Ok(satirlar.len() as u64)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::test_yardimcisi::GeciciDizin;

    fn basliklar(adlar: &[&str]) -> Vec<String> {
        adlar.iter().map(|a| (*a).to_string()).collect()
    }

    fn satir(no: u64, alanlar: &[&str]) -> Satir {
        Satir {
            no,
            alanlar: alanlar.iter().map(|a| (*a).to_string()).collect(),
            json: None,
            uyari: None,
        }
    }

    fn oku(yol: &Path) -> String {
        std::fs::read_to_string(yol).expect("oku")
    }

    #[test]
    fn bicim_adlari_ve_ayristirma() {
        assert_eq!(DisaBiimi::Csv.ad(), "csv");
        assert_eq!(DisaBiimi::ayristir("CSV"), Some(DisaBiimi::Csv));
        assert_eq!(DisaBiimi::ayristir("ndjson"), Some(DisaBiimi::Jsonl));
        assert_eq!(DisaBiimi::ayristir("parquet"), None);
    }

    #[test]
    fn duz_alan_kacis_gerekmez() {
        assert_eq!(csv_alan_kacis("ali"), "ali");
    }

    #[test]
    fn ayrac_icer_alan_tirnalanir() {
        assert_eq!(csv_alan_kacis("a,b"), "\"a,b\"");
    }

    #[test]
    fn tirnak_kacisi_uygulanir() {
        assert_eq!(csv_alan_kacis("de\"diye"), "\"de\"\"diye\"");
    }

    #[test]
    fn satir_sonu_icer_alan_tirnalanir() {
        assert_eq!(csv_alan_kacis("a\nb"), "\"a\nb\"");
    }

    #[test]
    fn bos_alan_tirnalanir() {
        assert_eq!(csv_alan_kacis(""), "\"\"");
    }

    #[test]
    fn bastaki_son_boşluk_tirnalanir() {
        assert_eq!(csv_alan_kacis(" ali"), "\" ali\"");
    }

    #[test]
    fn csv_disa_aktarma_baslik_ve_satirlar_yazar() {
        let d = GeciciDizin::yeni("dl-disa-csv").expect("dizin");
        let hedef = d.yol().join("out.csv");
        let s = vec![satir(0, &["ali", "30"]), satir(1, &["ayse", "41"])];
        let sonuc =
            disa_aktar(&hedef, DisaBiimi::Csv, &basliklar(&["ad", "yas"]), &s).expect("yaz");
        assert_eq!(sonuc.satir, 2);
        assert_eq!(sonuc.sutun, 2);
        assert_eq!(sonuc.bicim, "csv");
        let icerik = oku(&hedef);
        assert_eq!(icerik, "ad,yas\r\nali,30\r\nayse,41\r\n");
    }

    #[test]
    fn csv_disa_aktarma_tirnaklari_korur() {
        let d = GeciciDizin::yeni("dl-disa-tirnak").expect("dizin");
        let hedef = d.yol().join("out.csv");
        let s = vec![satir(0, &["a,b", "c\"d"])];
        disa_aktar(&hedef, DisaBiimi::Csv, &basliklar(&["x", "y"]), &s).expect("yaz");
        let icerik = oku(&hedef);
        assert!(icerik.contains("\"a,b\""));
        assert!(icerik.contains("\"c\"\"d\""));
    }

    #[test]
    fn jsonl_disa_aktarma_nesne_yazar() {
        let d = GeciciDizin::yeni("dl-disa-jsonl").expect("dizin");
        let hedef = d.yol().join("out.jsonl");
        let s = vec![satir(0, &["ali", "30"])];
        let sonuc =
            disa_aktar(&hedef, DisaBiimi::Jsonl, &basliklar(&["ad", "yas"]), &s).expect("yaz");
        assert_eq!(sonuc.bicim, "jsonl");
        let icerik = oku(&hedef);
        assert_eq!(icerik.trim(), r#"{"ad":"ali","yas":"30"}"#);
    }

    #[test]
    fn jsonl_disa_aktarma_ozgun_tipleri_korur() {
        let d = GeciciDizin::yeni("dl-disa-tipli").expect("dizin");
        let hedef = d.yol().join("out.jsonl");
        let s = vec![Satir {
            no: 0,
            alanlar: vec!["5".to_string(), "true".to_string()],
            json: Some(serde_json::json!({"n": 5, "o": true})),
            uyari: None,
        }];
        disa_aktar(&hedef, DisaBiimi::Jsonl, &basliklar(&["n", "o"]), &s).expect("yaz");
        let icerik = oku(&hedef);
        assert!(icerik.contains("\"n\":5"));
        assert!(icerik.contains("\"o\":true"));
    }

    #[test]
    fn gecici_dosya_yazma_sonrasina_kadar_kalmaz() {
        let d = GeciciDizin::yeni("dl-disa-gecici").expect("dizin");
        let hedef = d.yol().join("out.csv");
        let s = vec![satir(0, &["1"])];
        disa_aktar(&hedef, DisaBiimi::Csv, &basliklar(&["x"]), &s).expect("yaz");
        let gecici = d.yol().join("out.csv.datalens-ortaci");
        assert!(!gecici.exists(), "gecici dosya kaldi");
        assert!(hedef.exists());
    }

    #[test]
    fn olmayan_dizine_yazma_hata_verir() {
        let d = GeciciDizin::yeni("dl-disa-dizin").expect("dizin");
        let hedef = d.yol().join("olmayan").join("out.csv");
        let s = vec![satir(0, &["1"])];
        let hata = disa_aktar(&hedef, DisaBiimi::Csv, &basliklar(&["x"]), &s).expect_err("hata");
        assert!(matches!(hata, Hata::CiktiHatasi { .. }));
    }

    #[test]
    fn gidis_donus_degerleri_korur() {
        let d = GeciciDizin::yeni("dl-disa-gidis").expect("dizin");
        let baslik = basliklar(&["ad", "not"]);
        let s = vec![
            satir(0, &["ali", "ilk, okul"]),
            satir(1, &["ayse", "de\"diye"]),
        ];
        let hedef = d.yol().join("out.csv");
        disa_aktar(&hedef, DisaBiimi::Csv, &baslik, &s).expect("yaz");

        // Ayni dosyayi tekrar okuyup degerlerin ayni kaldigini dogrula.
        let ayarlar = crate::kaynak::Ayarlar::default();
        let mut kaynak = crate::kaynak::Kaynak::ac(&hedef, ayarlar).expect("ac");
        assert_eq!(kaynak.basliklar(), &baslik);
        let tekrar = kaynak.pencere(0, 2).expect("pencere");
        assert_eq!(tekrar[0].alanlar, s[0].alanlar);
        assert_eq!(tekrar[1].alanlar, s[1].alanlar);
        assert!(tekrar.iter().all(|x| x.uyari.is_none()));
    }
}
