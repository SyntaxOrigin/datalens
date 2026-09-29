//! Sütun tipi çıkarımı ve tekil değer sınıflandırması.
//!
//! Sorumluluğu, ham metin alanlarını anlamlı türlere sınıflandırmaktır:
//! boş, mantıksal, tam sayı, kayan nokta, tarih ve metin.
//!
//! Tipler bir **toplam sıralama** üzerinde yükseltilir:
//!
//! ```text
//! Bos < Mantiksal < TamSayi < KayanNokta < Tarih < Metin
//! ```
//!
//! Bu sıralama bir alt küme ilişkisi değil, yalnızca "daha geniş tiye doğru
//! ilerleme" yönünü tanımlar. Örneğin `true` ve `1` aynı sütunda bulunursa
//! sütun `tam_sayi` olur; bu bir belirsizliktir ve `## Bilinen Sınırlamalar`
//! bölümünde belgelenmiştir.

use serde::Serialize;

/// Bir sütunun çıkarılan tipi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SutunTuru {
    /// Örneklemde dolu değer yok.
    Bos,
    /// Yalnızca `true` / `false` (büyük/küçük harf duyarsız).
    Mantiksal,
    /// Tüm dolu değerler `i64` olarak ayrıştırılabilir.
    TamSayi,
    /// Tüm dolu değerler sonlu `f64` olarak ayrıştırılabilir.
    KayanNokta,
    /// Tüm dolu değerler desteklenen tarih biçimlerinden birine uyar.
    Tarih,
    /// Hiçbir daraltma uygulanamaz.
    Metin,
}

impl SutunTuru {
    /// Yükseltme sırasındaki sıra numarası.
    pub fn genislik(self) -> u8 {
        match self {
            SutunTuru::Bos => 0,
            SutunTuru::Mantiksal => 1,
            SutunTuru::TamSayi => 2,
            SutunTuru::KayanNokta => 3,
            SutunTuru::Tarih => 4,
            SutunTuru::Metin => 5,
        }
    }

    /// İki tip arasındaki en genişini döndürür.
    pub fn birlestir(self, diger: SutunTuru) -> SutunTuru {
        if diger.genislik() > self.genislik() {
            diger
        } else {
            self
        }
    }

    /// Tipin adı; JSON çıktısında ve hata mesajlarında kullanılır.
    pub fn ad(self) -> &'static str {
        match self {
            SutunTuru::Bos => "bos",
            SutunTuru::Mantiksal => "mantiksal",
            SutunTuru::TamSayi => "tam_sayi",
            SutunTuru::KayanNokta => "kayan_nokta",
            SutunTuru::Tarih => "tarih",
            SutunTuru::Metin => "metin",
        }
    }

    /// Tip sayısal mı?
    pub fn sayisal(self) -> bool {
        matches!(self, SutunTuru::TamSayi | SutunTuru::KayanNokta)
    }
}

/// Değerin boş olup olmadığını söyler.
pub fn bos_mu(deger: &str) -> bool {
    deger.is_empty()
}

/// Değer `true` / `false` literal eşdeğeri mi?
pub fn mantiksal_mi(deger: &str) -> bool {
    deger.eq_ignore_ascii_case("true") || deger.eq_ignore_ascii_case("false")
}

/// Değer `i64` olarak ayrıştırılabiliyor mu?
pub fn tam_sayi_mi(deger: &str) -> bool {
    deger.parse::<i64>().is_ok()
}

/// Değer **sonlu** `f64` olarak ayrıştırılabiliyor mu?
///
/// `inf`, `-inf` ve `NaN` kabul edilmez: bu dizeler istatistik hesabını
/// bozacağı için metin kabul edilirler.
pub fn kayan_mi(deger: &str) -> bool {
    match deger.parse::<f64>() {
        Ok(v) => v.is_finite(),
        Err(_) => false,
    }
}

/// Değeri sonlu `f64` olarak çevirir; çevrilemiyorsa `None` döner.
pub fn sayiya(deger: &str) -> Option<f64> {
    match deger.parse::<f64>() {
        Ok(v) if v.is_finite() => Some(v),
        _ => None,
    }
}

/// Yalnızca rakamlardan oluşan ve verilen uzunlukta bir dilimi doğrular.
fn rakamlar(kaynak: &[u8]) -> bool {
    !kaynak.is_empty() && kaynak.iter().all(|b| b.is_ascii_digit())
}

/// Tarih benzeri ama geçersiz (ör. `2024-13-45`) bir değer olup olmadığını
/// söyler.
///
/// Bu ayrım, raporun S4 kabul kriterindeki "farklı biçimler tutarsız olarak
/// listelenir" davranışını mümkün kılar: biçimi tanınan ama geçersiz değerler
/// sessizce yutulmaz, `tarih_tutarsiz` sayacına girer.
pub fn tarih_kesisimi_mi(deger: &str) -> bool {
    let b = deger.as_bytes();
    if b.len() != 10 {
        return false;
    }
    // YYYY-AA-GG
    if b[4] != b'-' && b[4] != b'.' && b[4] != b'/' {
        return false;
    }
    if b[7] != b'-' && b[7] != b'.' && b[7] != b'/' {
        return false;
    }
    rakamlar(&b[0..4]) && rakamlar(&b[5..7]) && rakamlar(&b[8..10])
}

/// Tarih değerini sıralanabilir `YYYY-AA-GG` anahtarına çevirir.
///
/// Desteklenen biçimler: `YYYY-AA-GG`, `GG.AA.YYYY`, `GG-AA-YYYY` ve bunların
/// ardından gelen isteğe bağlı saat kısmı (`T` veya boşluk ayracı).
/// Anahtar, farklı biçimlerin aynı sıralamada karşılaştırılabilmesini sağlar.
pub fn tarih_anahtari(deger: &str) -> Option<String> {
    let b = deger.as_bytes();
    if b.len() < 10 {
        return None;
    }
    let (yil, ay, gun) = if rakamlar(&b[0..4]) && (b[4] == b'-' || b[4] == b'/' || b[4] == b'.') {
        (&b[0..4], &b[5..7], &b[8..10])
    } else if (b[2] == b'-' || b[2] == b'.' || b[2] == b'/')
        && (b[5] == b'-' || b[5] == b'.' || b[5] == b'/')
    {
        (&b[6..10], &b[3..5], &b[0..2])
    } else {
        return None;
    };
    let yil_s: Option<u32> = std::str::from_utf8(yil).ok()?.parse().ok();
    let ay_s: Option<u32> = std::str::from_utf8(ay).ok()?.parse().ok();
    let gun_s: Option<u32> = std::str::from_utf8(gun).ok()?.parse().ok();
    let (yil_s, ay_s, gun_s) = (yil_s?, ay_s?, gun_s?);
    if !(1..=12).contains(&ay_s) || !(1..=31).contains(&gun_s) {
        return None;
    }
    let saat = saat_anahtari(&deger[10..])?;
    Some(format!("{yil_s:04}-{ay_s:02}-{gun_s:02}{saat}"))
}

/// Tarih sonrasındaki saat kısmını sıralanabilir biçime çevirir.
///
/// Desteklenenler: `HH:MM`, `HH:MM:SS`, `HH:MM:SS.mmm`, ve bunların `Z` veya
/// `+HH:MM` / `-HH:MM` son ekleri. Saat yoksa boş dize döner.
fn saat_anahtari(kalan: &str) -> Option<String> {
    let kalan = kalan.trim();
    if kalan.is_empty() {
        return Some(String::new());
    }
    let saat_durum = kalan
        .split(['T', 't', ' '])
        .find(|p| !p.is_empty())
        .unwrap_or("");
    if saat_durum.is_empty() {
        return Some(String::new());
    }
    let b = saat_durum.as_bytes();
    if b.len() < 5 || b[2] != b':' {
        return None;
    }
    let saat: u32 = std::str::from_utf8(&b[0..2]).ok()?.parse().ok()?;
    let dakika: u32 = std::str::from_utf8(&b[3..5]).ok()?.parse().ok()?;
    if saat > 23 || dakika > 59 {
        return None;
    }
    let mut sonuc = format!("T{saat:02}:{dakika:02}");
    if b.len() > 5 && b[5] == b':' {
        if b.len() < 8 {
            return None;
        }
        let saniye: u32 = std::str::from_utf8(&b[6..8]).ok()?.parse().ok()?;
        if saniye > 60 {
            return None;
        }
        sonuc.push_str(&format!(":{saniye:02}"));
        if b.len() > 8 {
            let kalan_saat = &saat_durum[8..];
            if let Some(son) = kalan_saat.strip_prefix('.') {
                let rakam = son.chars().take_while(|c| c.is_ascii_digit()).count();
                if rakam == 0 {
                    return None;
                }
                sonuc.push('.');
                sonuc.push_str(&format!("{:0<3}", &son[..rakam]));
            } else if !restsayi_gecerli(kalan_saat) {
                return None;
            }
        }
    }
    if b.len() > 5 && (b[5] == b'Z' || b[5] == b'z') {
        sonuc.push('Z');
    } else if let Some(ofset) = kalan_saat_ofseti(saat_durum) {
        sonuc.push_str(ofset.as_str());
    }
    Some(sonuc)
}

/// Saat dizesinin sonundaki `Z` veya `±HH:MM` son ekini doğrular ve döndürür.
fn kalan_saat_ofseti(saat_durum: &str) -> Option<String> {
    let konum = saat_durum
        .rfind(['+', '-'])
        .filter(|i| *i >= 5)
        .or_else(|| saat_durum.find(['Z', 'z']).filter(|i| *i >= 5));
    let konum = konum?;
    if restsayi_gecerli(&saat_durum[konum..]) {
        Some(saat_durum[konum..].to_ascii_uppercase())
    } else {
        None
    }
}

/// Bir saat son ekinin (`Z`, `+02:00`, `-05:30`) biçimini doğrular.
fn restsayi_gecerli(son: &str) -> bool {
    if son.eq_ignore_ascii_case("Z") {
        return true;
    }
    let b = son.as_bytes();
    b.len() == 6
        && (b[0] == b'+' || b[0] == b'-')
        && rakamlar(&b[1..3])
        && b[3] == b':'
        && rakamlar(&b[4..6])
}

/// Tek bir değerin kendi tipini döndürür.
pub fn deger_turu(deger: &str) -> SutunTuru {
    if bos_mu(deger) {
        SutunTuru::Bos
    } else if mantiksal_mi(deger) {
        SutunTuru::Mantiksal
    } else if tam_sayi_mi(deger) {
        SutunTuru::TamSayi
    } else if kayan_mi(deger) {
        SutunTuru::KayanNokta
    } else if tarih_anahtari(deger).is_some() {
        SutunTuru::Tarih
    } else {
        SutunTuru::Metin
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn bos_deger_turu_bostur() {
        assert_eq!(deger_turu(""), SutunTuru::Bos);
        assert!(bos_mu(""));
        assert!(!bos_mu(" "));
    }

    #[test]
    fn mantiksal_degerler_taninir() {
        assert_eq!(deger_turu("true"), SutunTuru::Mantiksal);
        assert_eq!(deger_turu("FALSE"), SutunTuru::Mantiksal);
        assert!(mantiksal_mi("True"));
        assert!(!mantiksal_mi("yes"));
    }

    #[test]
    fn tam_sayi_degerleri_taninir() {
        assert_eq!(deger_turu("42"), SutunTuru::TamSayi);
        assert_eq!(deger_turu("-7"), SutunTuru::TamSayi);
        assert_eq!(deger_turu("+3"), SutunTuru::TamSayi);
        assert!(tam_sayi_mi("0012"));
        assert!(!tam_sayi_mi("1.0"));
        assert!(!tam_sayi_mi("1_000"));
    }

    #[test]
    fn kayan_nokta_degerleri_taninir() {
        assert_eq!(deger_turu("3.5"), SutunTuru::KayanNokta);
        assert!(kayan_mi("1e5"));
        assert!(kayan_mi(".5"));
        assert!(!kayan_mi("inf"));
        assert!(!kayan_mi("NaN"));
        assert_eq!(sayiya("2.5"), Some(2.5));
        assert_eq!(sayiya("abc"), None);
    }

    #[test]
    fn tarih_degerleri_taninir() {
        assert_eq!(deger_turu("2024-03-15"), SutunTuru::Tarih);
        assert_eq!(deger_turu("15.03.2024"), SutunTuru::Tarih);
        assert_eq!(deger_turu("15-03-2024"), SutunTuru::Tarih);
        assert_eq!(deger_turu("2024-03-15T10:30"), SutunTuru::Tarih);
    }

    #[test]
    fn gecersiz_tarih_tarih_sayilmaz() {
        assert!(tarih_anahtari("2024-13-01").is_none());
        assert!(tarih_anahtari("2024-03-32").is_none());
        assert!(tarih_anahtari("2024-03").is_none());
        assert!(deger_turu("2024-13-45") == SutunTuru::Metin);
    }

    #[test]
    fn tarih_kesisimi_gecersiz_degerleri_yakalar() {
        assert!(tarih_kesisimi_mi("2024-13-45"));
        assert!(tarih_kesisimi_mi("2024-03-15"));
        assert!(!tarih_kesisimi_mi("merhaba"));
    }

    #[test]
    fn tarih_anahtarlari_siralanabilir_ve_bicimden_bagimsiz() {
        assert_eq!(tarih_anahtari("2024-03-15").unwrap(), "2024-03-15");
        assert_eq!(tarih_anahtari("15.03.2024").unwrap(), "2024-03-15");
        assert_eq!(
            tarih_anahtari("2024-03-15T09:05:03").unwrap(),
            "2024-03-15T09:05:03"
        );
        assert_eq!(
            tarih_anahtari("2024-03-15T09:05:03.25").unwrap(),
            "2024-03-15T09:05:03.250"
        );
        assert_eq!(
            tarih_anahtari("2024-03-15T09:05Z").unwrap(),
            "2024-03-15T09:05Z"
        );
        assert_eq!(
            tarih_anahtari("2024-03-15T09:05+02:00").unwrap(),
            "2024-03-15T09:05+02:00"
        );
        assert_eq!(tarih_anahtari("2024-03-15T25:00"), None);
    }

    #[test]
    fn tip_yukseltme_toplam_siralama_yapar() {
        let mut tur = SutunTuru::Bos;
        for deger in ["1", "2", "3"] {
            tur = tur.birlestir(deger_turu(deger));
        }
        assert_eq!(tur, SutunTuru::TamSayi);

        tur = tur.birlestir(deger_turu("4.5"));
        assert_eq!(tur, SutunTuru::KayanNokta);

        tur = tur.birlestir(deger_turu("bes"));
        assert_eq!(tur, SutunTuru::Metin);
    }

    #[test]
    fn tip_adlari_ve_sayisallik_bilgisi() {
        assert_eq!(SutunTuru::TamSayi.ad(), "tam_sayi");
        assert!(SutunTuru::TamSayi.sayisal());
        assert!(SutunTuru::KayanNokta.sayisal());
        assert!(!SutunTuru::Tarih.sayisal());
        assert!(!SutunTuru::Metin.sayisal());
        assert!(!SutunTuru::Bos.sayisal());
        assert!(!SutunTuru::Mantiksal.sayisal());
    }

    #[test]
    fn bos_tur_diger_turlerle_birlestirilince_diger_kazanir() {
        assert_eq!(SutunTuru::Bos.birlestir(SutunTuru::Metin), SutunTuru::Metin);
        assert_eq!(SutunTuru::Metin.birlestir(SutunTuru::Bos), SutunTuru::Metin);
    }

    #[test]
    fn json_serilestirme_snake_case_uretir() {
        let metin = serde_json::to_string(&SutunTuru::KayanNokta).unwrap();
        assert_eq!(metin, "\"kayan_nokta\"");
    }
}
