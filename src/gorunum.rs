//! Terminal tabanlı tablo gösterimi.
//!
//! Sorumluluğu, sütun adları ve satırları okunaklı bir metin tablosuna
//! dönüştürmektir. Raporda sanal kaydırmalı ızgara bir pencere katmanıyla
//! gösterilir; karar D-010 gereği pencere katmanı yasak olduğundan buradaki
//! çözüm **pencere boyutunda satır basmak**tır: yalnızca istenen satırlar
//! okunur ve yalnızca onlar basılır.
//!
//! Ne **değil**: bu modül dosya okumaz, filtre uygulamaz, veri kaybına yol açan
//! kırpma yapmaz — kırpma yalnızca **görüntülemededir** ve `…` işaretiyle
//! belirtilir.

use std::io::{self, Write};

use crate::kaynak::Satir;

/// Hücre kırpılırken sona eklenen işaret.
const KIRPMA_ISARETI: char = '\u{2026}';
/// Satır numarası sütununun sabit genişliği.
const SATIR_NO_GENISLIK: usize = 8;
/// Sütunlar arası ayracın (` | `) genişliği.
const AYRAC_GENISLIK: usize = 3;
/// Bir sütunun asgari genişliği.
const ASGARI_SUTUN: usize = 3;

/// Tablo çizim ayarları.
#[derive(Debug, Clone, Copy)]
pub struct GorunumAyar {
    /// Bir satırın azami toplam genişliği ( karakter).
    pub genislik: usize,
    /// Bir sütunun azami genişliği ( karakter).
    pub sutun_genislik: usize,
}

impl Default for GorunumAyar {
    fn default() -> Self {
        GorunumAyar {
            genislik: 200,
            sutun_genislik: 24,
        }
    }
}

/// Kontrol karakterlerini tek boşluğa indirger ve hücreyi tek satırda tutar.
fn tek_satir(deger: &str) -> String {
    let mut sonuc = String::with_capacity(deger.len());
    let mut onceki_bosluk = false;
    for karakter in deger.chars() {
        let bosluk = karakter.is_control() || karakter == '\u{feff}';
        if bosluk {
            if !onceki_bosluk {
                sonuc.push(' ');
            }
            onceki_bosluk = true;
        } else {
            sonuc.push(karakter);
            onceki_bosluk = false;
        }
    }
    sonuc.trim().to_string()
}

/// Bir hücreyi verilen genişliğe sığdırır; kırpıldıysa `…` ekler.
pub fn hucre_sigdir(deger: &str, genislik: usize) -> String {
    let metin = tek_satir(deger);
    if genislik == 0 {
        return String::new();
    }
    if metin.chars().count() <= genislik {
        return metin;
    }
    let son = metin
        .chars()
        .take(genislik.saturating_sub(1))
        .collect::<String>();
    format!("{son}{KIRPMA_ISARETI}")
}

/// Hücreyi verilen genişliğe sağa dayalı olarak yazar.
fn hucre_yaz(yazici: &mut impl Write, metin: &str, genislik: usize) -> io::Result<()> {
    let dolu = metin.chars().count();
    for _ in dolu..genislik {
        write!(yazici, " ")?;
    }
    write!(yazici, "{metin}")
}

/// Görüntülenecek sütunların indekslerini belirler.
///
/// `secim` boşsa tüm sütunlar, doluysa yalnızca verilenler kullanılır. Verilen
/// indeks sıra dışıysa yok sayılır; böylece hataya yol açmaz.
pub fn sutun_sec(basliklar: &[String], secim: &[usize]) -> Vec<usize> {
    if secim.is_empty() {
        return (0..basliklar.len()).collect();
    }
    secim
        .iter()
        .copied()
        .filter(|i| *i < basliklar.len())
        .collect()
}

/// Sütun genişliklerini hesaplar ve toplam genişliğe sığdırır.
///
/// Genişlik bütçesi `ayar.genislik` değeridir; ancak taban genişlik
/// (`SATIR_NO_GENISLIK + sütun sayısı × (AYRAC + ASGARI_SUTUN)`) bütçeden
/// büyükse taban genişlik uygulanır — hiçbir sütun okunamaz hâle getirilmez.
/// Uyarı sütunu varsa bütçeye `AYRAC_GENISLIK + 1` eklenir.
pub fn sutun_genislikleri(
    basliklar: &[String],
    sutunlar: &[usize],
    satirlar: &[Satir],
    ayar: &GorunumAyar,
    uyarli: bool,
) -> Vec<usize> {
    let en_fazla = ayar.sutun_genislik.max(ASGARI_SUTUN);
    let sayi = sutunlar.len();
    let ekstra = if uyarli { AYRAC_GENISLIK + 1 } else { 0 };
    let taban = SATIR_NO_GENISLIK + sayi * (AYRAC_GENISLIK + ASGARI_SUTUN) + ekstra;
    let butce = ayar.genislik.max(taban);

    let mut genislikler: Vec<usize> = sutunlar
        .iter()
        .map(|i| {
            basliklar
                .get(*i)
                .map(|b| b.chars().count())
                .unwrap_or(0)
                .clamp(ASGARI_SUTUN, en_fazla)
        })
        .collect();
    for satir in satirlar {
        for (sira, sutun) in sutunlar.iter().enumerate() {
            if let Some(deger) = satir.alanlar.get(*sutun) {
                let olcu = deger.chars().count().clamp(ASGARI_SUTUN, en_fazla);
                if olcu > genislikler[sira] {
                    genislikler[sira] = olcu;
                }
            }
        }
    }

    // En genis sutunlardan daralt; hicbir sutun asgari genisligin altina inmez.
    let sabit = SATIR_NO_GENISLIK + sayi * AYRAC_GENISLIK + ekstra;
    let mut toplam: usize = genislikler.iter().sum::<usize>() + sabit;
    while toplam > butce {
        let en_genis = genislikler
            .iter()
            .enumerate()
            .filter(|(_, g)| **g > ASGARI_SUTUN)
            .max_by_key(|(_, g)| **g)
            .map(|(i, _)| i);
        match en_genis {
            Some(i) => {
                genislikler[i] -= 1;
                toplam -= 1;
            }
            None => break,
        }
    }
    genislikler
}

/// Tabloyu yazıcıya basar.
///
/// İlk sütun satır numarasıdır; hücreler verilen genişliğe kırpılır. Satırda
/// uyarı varsa son sütun olarak `!` işaretlenir ve uyarı metni parantez içinde
/// gösterilir.
pub fn tablo_yaz(
    yazici: &mut impl Write,
    basliklar: &[String],
    sutunlar: &[usize],
    satirlar: &[Satir],
    ayar: &GorunumAyar,
) -> io::Result<()> {
    let sutunlar = sutun_sec(basliklar, sutunlar);
    if sutunlar.is_empty() {
        writeln!(yazici, "(bu tabloda gosterilecek sutun yok)")?;
        return Ok(());
    }
    let uyarli = satirlar.iter().any(|s| s.uyari.is_some());
    let genislikler = sutun_genislikleri(basliklar, &sutunlar, satirlar, ayar, uyarli);

    write!(yazici, "{:>width$}", "#", width = SATIR_NO_GENISLIK)?;
    for (sira, sutun) in sutunlar.iter().enumerate() {
        write!(yazici, " | ")?;
        let baslik = basliklar.get(*sutun).cloned().unwrap_or_default();
        hucre_yaz(
            yazici,
            &hucre_sigdir(&baslik, genislikler[sira]),
            genislikler[sira],
        )?;
    }
    if uyarli {
        write!(yazici, " | !")?;
    }
    writeln!(yazici)?;

    write!(yazici, "{}", "-".repeat(SATIR_NO_GENISLIK))?;
    for genislik in &genislikler {
        write!(yazici, "-+-{}", "-".repeat(*genislik))?;
    }
    if uyarli {
        write!(yazici, "-+-")?;
    }
    writeln!(yazici)?;

    for satir in satirlar {
        write!(yazici, "{:>width$}", satir.no, width = SATIR_NO_GENISLIK)?;
        for (sira, sutun) in sutunlar.iter().enumerate() {
            write!(yazici, " | ")?;
            let deger = satir.alanlar.get(*sutun).map(String::as_str).unwrap_or("?");
            hucre_yaz(
                yazici,
                &hucre_sigdir(deger, genislikler[sira]),
                genislikler[sira],
            )?;
        }
        if uyarli {
            write!(yazici, " | ")?;
            let isaret = if satir.uyari.is_some() { "!" } else { "" };
            hucre_yaz(yazici, isaret, 1)?;
        }
        writeln!(yazici)?;
    }
    Ok(())
}

/// Satır uyarılarını ayrı bir bölüm olarak yazar.
pub fn uyarilari_yaz(yazici: &mut impl Write, satirlar: &[Satir]) -> io::Result<()> {
    let uyarili: Vec<&Satir> = satirlar.iter().filter(|s| s.uyari.is_some()).collect();
    if uyarili.is_empty() {
        return Ok(());
    }
    writeln!(yazici, "uyarilar:")?;
    for satir in uyarili {
        writeln!(
            yazici,
            "  #{} {}",
            satir.no,
            satir.uyari.as_deref().unwrap_or_default()
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

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

    fn yaz(basliklar: &[String], satirlar: &[Satir]) -> String {
        let mut tampon: Vec<u8> = Vec::new();
        let ayar = GorunumAyar::default();
        tablo_yaz(&mut tampon, basliklar, &[], satirlar, &ayar).expect("yaz");
        String::from_utf8(tampon).expect("utf8")
    }

    #[test]
    fn kisa_hucre_degistirilmez() {
        assert_eq!(hucre_sigdir("abc", 10), "abc");
    }

    #[test]
    fn uzun_hucre_kirpılir_ve_isaretlenir() {
        let sonuc = hucre_sigdir("abcdefghij", 5);
        assert_eq!(sonuc.chars().count(), 5);
        assert!(sonuc.ends_with(KIRPMA_ISARETI));
    }

    #[test]
    fn sifir_genislikte_hucre_bos_doner() {
        assert_eq!(hucre_sigdir("abc", 0), "");
    }

    #[test]
    fn kontrol_karakterleri_tek_satira_indirgenir() {
        assert_eq!(tek_satir("a\nb\tc\rd"), "a b c d");
    }

    #[test]
    fn sutun_secimi_bos_ise_tumunu_secer() {
        let b = basliklar(&["a", "b", "c"]);
        assert_eq!(sutun_sec(&b, &[]), vec![0, 1, 2]);
    }

    #[test]
    fn sutun_secimi_verilenleri_kullanir() {
        let b = basliklar(&["a", "b", "c"]);
        assert_eq!(sutun_sec(&b, &[2, 0]), vec![2, 0]);
    }

    #[test]
    fn gecersiz_sutun_secimi_yok_sayilir() {
        let b = basliklar(&["a", "b"]);
        assert_eq!(sutun_sec(&b, &[0, 9]), vec![0]);
    }

    #[test]
    fn sutun_secimi_bos_kalirsa_mesaj_yazar() {
        let mut tampon: Vec<u8> = Vec::new();
        let ayar = GorunumAyar::default();
        tablo_yaz(&mut tampon, &[], &[], &[], &ayar).expect("yaz");
        assert!(String::from_utf8(tampon).unwrap().contains("sutun yok"));
    }

    #[test]
    fn tablo_baslik_ayrac_ve_satirlari_yazar() {
        let b = basliklar(&["ad", "yas"]);
        let s = vec![satir(0, &["ali", "30"]), satir(1, &["ayse", "41"])];
        let metin = yaz(&b, &s);
        let satirlar: Vec<&str> = metin.lines().collect();
        assert_eq!(satirlar.len(), 4, "baslik + ayrac + 2 satir");
        assert!(satirlar[0].contains("ad"));
        assert!(satirlar[0].contains("yas"));
        assert!(satirlar[2].contains("ali"));
        assert!(satirlar[3].contains("ayse"));
    }

    #[test]
    fn eksik_sutun_soru_isaretiyle_gosterilir() {
        let b = basliklar(&["a", "b", "c"]);
        let s = vec![satir(0, &["1", "2"])];
        let metin = yaz(&b, &s);
        assert!(metin.contains('?'));
    }

    #[test]
    fn uyarli_satirda_isaret_sutunu_eklenir() {
        let b = basliklar(&["a"]);
        let mut s = satir(0, &["1"]);
        s.uyari = Some("kapatilmamis tirnak".to_string());
        let metin = yaz(&b, &[s]);
        assert!(metin.contains('!'));
    }

    #[test]
    fn uyari_yoksa_isaret_sutunu_eklenmez() {
        let b = basliklar(&["a"]);
        let metin = yaz(&b, &[satir(0, &["1"])]);
        assert!(!metin.contains('!'));
    }

    #[test]
    fn uyarilari_yaz_yalnizca_uyarili_satirlari_basar() {
        let mut tampon: Vec<u8> = Vec::new();
        let s1 = satir(0, &["1"]);
        let mut s2 = satir(1, &["2"]);
        s2.uyari = Some("bozuk JSON".to_string());
        uyarilari_yaz(&mut tampon, &[s1, s2]).expect("yaz");
        let metin = String::from_utf8(tampon).unwrap();
        assert_eq!(metin.lines().count(), 2);
        assert!(metin.contains("bozuk JSON"));
    }

    #[test]
    fn uyari_yoksa_uyarilari_yaz_bos_kalir() {
        let mut tampon: Vec<u8> = Vec::new();
        uyarilari_yaz(&mut tampon, &[satir(0, &["1"])]).expect("yaz");
        assert!(tampon.is_empty());
    }

    #[test]
    fn sutun_genislikleri_basliga_gore_buyur() {
        let b = basliklar(&["kisa", "cok_uzun_bir_baslik_adi"]);
        let g = sutun_genislikleri(&b, &[0, 1], &[], &GorunumAyar::default(), false);
        assert!(g[0] >= ASGARI_SUTUN);
        assert!(g[1] > g[0]);
    }

    #[test]
    fn sutun_genisligi_asgari_degerin_altina_inmez() {
        let b = basliklar(&["a", "b", "c", "d"]);
        let ayar = GorunumAyar {
            genislik: 10,
            sutun_genislik: 30,
        };
        let g = sutun_genislikleri(&b, &[0, 1, 2, 3], &[], &ayar, false);
        assert!(g.iter().all(|x| *x >= ASGARI_SUTUN));
    }

    #[test]
    fn toplam_genislik_butceye_sigar() {
        let b = basliklar(&["a", "b", "c", "d", "e", "f", "g", "h"]);
        let uzun: Vec<Satir> = (0..3)
            .map(|i| {
                satir(
                    i,
                    &[
                        "0123456789",
                        "0123456789",
                        "0123456789",
                        "x",
                        "y",
                        "z",
                        "q",
                        "w",
                    ],
                )
            })
            .collect();
        let sutunlar = sutun_sec(&b, &[]);
        let ayar = GorunumAyar {
            genislik: 60,
            sutun_genislik: 20,
        };
        let g = sutun_genislikleri(&b, &sutunlar, &uzun, &ayar, false);
        let toplam: usize =
            g.iter().sum::<usize>() + SATIR_NO_GENISLIK + sutunlar.len() * AYRAC_GENISLIK;
        assert!(toplam <= 60, "toplam {toplam} 60'tan buyuk");
    }

    #[test]
    fn uyarli_sutun_butceye_dahildir() {
        let b = basliklar(&["a", "b"]);
        let mut s = satir(0, &["1", "2"]);
        s.uyari = Some("uyari".to_string());
        let ayar = GorunumAyar {
            genislik: 40,
            sutun_genislik: 10,
        };
        let g = sutun_genislikleri(&b, &[0, 1], &[s], &ayar, true);
        let toplam: usize =
            g.iter().sum::<usize>() + SATIR_NO_GENISLIK + 2 * AYRAC_GENISLIK + AYRAC_GENISLIK + 1;
        assert!(toplam <= 40, "toplam {toplam} 40'tan buyuk");
    }

    #[test]
    fn dar_genislikte_dikey_kisaltma_uygulanir() {
        let b = basliklar(&["a", "b", "c", "d"]);
        let s = vec![satir(
            0,
            &["0123456789", "0123456789", "0123456789", "0123456789"],
        )];
        let ayar = GorunumAyar {
            genislik: 40,
            sutun_genislik: 20,
        };
        let metin = {
            let mut tampon: Vec<u8> = Vec::new();
            tablo_yaz(&mut tampon, &b, &[], &s, &ayar).expect("yaz");
            String::from_utf8(tampon).unwrap()
        };
        for satir in metin.lines() {
            assert!(
                satir.chars().count() <= 40,
                "satir cok uzun: {}",
                satir.chars().count()
            );
        }
        assert!(metin.contains(KIRPMA_ISARETI), "kisaltma isareti yok");
    }
}
