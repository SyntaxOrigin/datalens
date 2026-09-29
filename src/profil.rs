//! Sütun profili ve şema çıkarımı.
//!
//! Sorumluluğu, örneklenmiş satırlardan sütun başına istatistik üretmektir:
//! çıkarılan tip, boş değer oranı, sayısal min/max/ortalama, tarih aralığı ve
//! en sık değerler.
//!
//! Örnekleme bilinçlidir: rapor (b05) "ilk N satır" yaklaşımının
//! yetersizliğini belirtir, bu yüzden `ornek_satir` sayısı yapılandırılabilir
//! ve **tüm dosyada boş kalan sütunların oranı** raporlanabilir. Varsayılan
//! 1000 satırdır (MANIFEST Kart 26, madde 4).
//!
//! Bellek sınırı: en sık değerler için tutulan ayrık anahtar sayısı
//! [`AYRIK_SINIR`] ile sınırlıdır. Sınır aşılırsa `farkli_deger_sinirda`
//! işareti yükselir; hafıza sessizce büyümez.

use std::collections::HashMap;

use serde::Serialize;

use crate::kaynak::{DosyaBilgisi, Satir};
use crate::tip::{bos_mu, sayiya, tarih_anahtari, tarih_kesisimi_mi, SutunTuru};

/// Bir sütun için tutulan ayrık değer anahtarı sayısının üst sınırı.
pub const AYRIK_SINIR: usize = 4096;

/// Gösterilecek en sık değer sayısı.
pub const EN_SIK_ADET: usize = 5;

/// Sayısal sütun istatistiği.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SayisalIstatistik {
    /// En küçük değer.
    pub min: f64,
    /// En büyük değer.
    pub max: f64,
    /// Ortalama. `f64` biriktirildiği için son ondalıkta yuvarlama farkı olabilir.
    pub ortalama: f64,
    /// Sayısal olarak sayılan değer sayısı.
    pub adet: u64,
}

/// Bir değerin sıklığı.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DegerSayisi {
    /// Değerin metin gösterimi.
    pub deger: String,
    /// Kaç kez geçtiği.
    pub adet: u64,
}

/// Tek bir sütunun profili.
#[derive(Debug, Clone, Serialize)]
pub struct SutunProfili {
    /// Sütunun sırası (sıfırdan başlar).
    pub sira: usize,
    /// Çözümlenen sütun adı.
    pub ad: String,
    /// Çıkarılan tip.
    pub tur: SutunTuru,
    /// Boş değer oranı (0.0 - 1.0).
    pub bos_oran: f64,
    /// Boş değer sayısı.
    pub bos_sayisi: u64,
    /// Dolu değer sayısı.
    pub dolu_sayisi: u64,
    /// Örneklemde görülen ayrık değer sayısı (sınıra takılırsa tam değildir).
    pub farkli_deger_sayisi: usize,
    /// Ayrık değer sınırı aşıldı mı?
    pub farkli_deger_sinirda: bool,
    /// En sık geçen değerler, sıklığa göre azalan.
    pub en_sik: Vec<DegerSayisi>,
    /// Sayısal sütunlarda min/max/ortalama.
    pub sayisal: Option<SayisalIstatistik>,
    /// Tarih sütunlarında en küçük tarih (`YYYY-AA-GG` anahtarı).
    pub tarih_min: Option<String>,
    /// Tarih sütunlarında en büyük tarih (`YYYY-AA-GG` anahtarı).
    pub tarih_max: Option<String>,
    /// Biçimi tarihe benzeyen ama geçersiz olan değer sayısı.
    ///
    /// Raporun S4 kabul kriteri: aynı sütunda farklı biçimler görüldüğünde
    /// tutarsızlar listelenmelidir; burada sayı olarak raporlanır.
    pub tarih_tutarsiz: u64,
    /// Örneklemde görülen çeşitli metin varyantı sayısı (küçük/küçük harf duyarsız).
    pub bicim_varyanti: usize,
}

/// Dosyanın tamamına ait profil.
#[derive(Debug, Clone, Serialize)]
pub struct Profil {
    /// Dosyanın statik bilgileri.
    pub dosya: DosyaBilgisi,
    /// Veri satırı sayısı (başlık dahil değil).
    pub satir_sayisi: u64,
    /// Sütun sayısı.
    pub sutun_sayisi: usize,
    /// Profilin üretilmesi için örneklenen satır sayısı.
    pub ornek_satir_sayisi: u64,
    /// Örneklemde uyarı alan satır sayısı.
    pub uyarili_satir_sayisi: u64,
    /// İndeksleme sırasında atlanan tamamen boş satır sayısı.
    pub atlanan_bos_satir: u64,
    /// Sütun profilleri.
    pub sutunlar: Vec<SutunProfili>,
}

/// Tek bir sütun için biriktirilen ara durum.
struct SutunBiriktirici {
    ad: String,
    sira: usize,
    bos: u64,
    dolu: u64,
    tur: SutunTuru,
    sayisal_adet: u64,
    min_sayi: Option<f64>,
    maks_sayi: Option<f64>,
    toplam_sayi: f64,
    tarih_min: Option<String>,
    tarih_max: Option<String>,
    tarih_tutarsiz: u64,
    frekans: HashMap<String, u64>,
    frekans_dolu: bool,
    bicimler: HashMap<String, u64>,
}

impl SutunBiriktirici {
    fn yeni(ad: String, sira: usize) -> Self {
        SutunBiriktirici {
            ad,
            sira,
            bos: 0,
            dolu: 0,
            tur: SutunTuru::Bos,
            sayisal_adet: 0,
            min_sayi: None,
            maks_sayi: None,
            toplam_sayi: 0.0,
            tarih_min: None,
            tarih_max: None,
            tarih_tutarsiz: 0,
            frekans: HashMap::new(),
            frekans_dolu: false,
            bicimler: HashMap::new(),
        }
    }

    /// Tek bir alan değerini biriktirir.
    fn ekle(&mut self, deger: &str) {
        if bos_mu(deger) {
            self.bos += 1;
            return;
        }
        self.dolu += 1;
        self.tur = self.tur.birlestir(crate::tip::deger_turu(deger));

        if let Some(v) = sayiya(deger) {
            self.sayisal_adet += 1;
            self.toplam_sayi += v;
            self.min_sayi = Some(self.min_sayi.map_or(v, |m: f64| m.min(v)));
            self.maks_sayi = Some(self.maks_sayi.map_or(v, |m: f64| m.max(v)));
        } else if let Some(anahtar) = tarih_anahtari(deger) {
            self.tarih_min = Some(match self.tarih_min.take() {
                Some(mevcut) => mevcut.min(anahtar.clone()),
                None => anahtar.clone(),
            });
            self.tarih_max = Some(match self.tarih_max.take() {
                Some(mevcut) => mevcut.max(anahtar.clone()),
                None => anahtar.clone(),
            });
        } else if tarih_kesisimi_mi(deger) {
            self.tarih_tutarsiz += 1;
        }

        let kucuk = deger.to_lowercase();
        let sayac = self.bicimler.entry(kucuk).or_insert(0);
        *sayac = sayac.saturating_add(1);

        if self.frekans.len() < AYRIK_SINIR || self.frekans.contains_key(deger) {
            let g = self.frekans.entry(deger.to_string()).or_insert(0);
            *g = g.saturating_add(1);
        } else {
            self.frekans_dolu = true;
        }
    }

    /// Biriktirilen durumu çıktı yapısına çevirir.
    fn bitir(self) -> SutunProfili {
        let toplam = self.bos + self.dolu;
        let bos_oran = if toplam == 0 {
            0.0
        } else {
            self.bos as f64 / toplam as f64
        };
        let sayisal = if self.sayisal_adet == 0 {
            None
        } else {
            Some(SayisalIstatistik {
                min: self.min_sayi.unwrap_or(0.0),
                max: self.maks_sayi.unwrap_or(0.0),
                ortalama: self.toplam_sayi / self.sayisal_adet as f64,
                adet: self.sayisal_adet,
            })
        };
        let mut sirali: Vec<(String, u64)> = self.frekans.into_iter().collect();
        sirali.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let farkli_deger_sayisi = sirali.len();
        let en_sik = sirali
            .into_iter()
            .take(EN_SIK_ADET)
            .map(|(deger, adet)| DegerSayisi { deger, adet })
            .collect();
        SutunProfili {
            sira: self.sira,
            ad: self.ad,
            tur: self.tur,
            bos_oran,
            bos_sayisi: self.bos,
            dolu_sayisi: self.dolu,
            farkli_deger_sayisi,
            farkli_deger_sinirda: self.frekans_dolu,
            en_sik,
            sayisal,
            tarih_min: self.tarih_min,
            tarih_max: self.tarih_max,
            tarih_tutarsiz: self.tarih_tutarsiz,
            bicim_varyanti: self.bicimler.len(),
        }
    }
}

/// Sütun adlarından başlatıcı biriktiriciler kurar.
fn biriktiriciler(basliklar: &[String]) -> Vec<SutunBiriktirici> {
    basliklar
        .iter()
        .enumerate()
        .map(|(i, ad)| SutunBiriktirici::yeni(ad.clone(), i))
        .collect()
}

/// Verilen satırlardan sütun profili üretir.
///
/// `ornek_satir` sıfırdan büyükse yalnızca ilk o kadar satır incelenir; profil
/// çıktısında örneklem sayısı raporlanır.
pub fn profil_uret(
    dosya: &DosyaBilgisi,
    basliklar: &[String],
    satirlar: &[Satir],
    toplam_satir: u64,
    atlanan_bos_satir: u64,
) -> Profil {
    let mut sutunlar = biriktiriciler(basliklar);
    let mut uyarili = 0u64;
    for satir in satirlar {
        if satir.uyari.is_some() {
            uyarili += 1;
        }
        for (i, sutun) in sutunlar.iter_mut().enumerate() {
            let deger = satir.alanlar.get(i).map(String::as_str).unwrap_or("");
            sutun.ekle(deger);
        }
    }
    Profil {
        dosya: dosya.clone(),
        satir_sayisi: toplam_satir,
        sutun_sayisi: basliklar.len(),
        ornek_satir_sayisi: satirlar.len() as u64,
        uyarili_satir_sayisi: uyarili,
        atlanan_bos_satir,
        sutunlar: sutunlar.into_iter().map(SutunBiriktirici::bitir).collect(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn bilgi() -> DosyaBilgisi {
        DosyaBilgisi {
            yol: "a.csv".to_string(),
            ad: "a.csv".to_string(),
            boyut_bayt: 100,
            bicim: "csv",
            ayrac: Some(",".to_string()),
            baslik_var: true,
            indeks_kipi: "seyrek",
            blok_satir: 512,
            dizi_kok: false,
            indeks_bellek_bayt: 8,
        }
    }

    fn basliklar(adlar: &[&str]) -> Vec<String> {
        adlar.iter().map(|a| (*a).to_string()).collect()
    }

    fn satir(alanlar: &[&str]) -> Satir {
        Satir {
            no: 0,
            alanlar: alanlar.iter().map(|a| (*a).to_string()).collect(),
            json: None,
            uyari: None,
        }
    }

    fn profil_uret_ile(baslik: &[&str], satirlar: &[Satir]) -> Profil {
        let b = basliklar(baslik);
        profil_uret(&bilgi(), &b, satirlar, satirlar.len() as u64, 0)
    }

    #[test]
    fn tam_sayi_tipi_cikarilir() {
        let p = profil_uret_ile(&["n"], &[satir(&["1"]), satir(&["2"]), satir(&["-3"])]);
        assert_eq!(p.sutunlar[0].tur, SutunTuru::TamSayi);
        assert_eq!(p.sutunlar[0].tur.ad(), "tam_sayi");
    }

    #[test]
    fn kayan_noktaya_yukselir() {
        let p = profil_uret_ile(&["n"], &[satir(&["1"]), satir(&["2.5"])]);
        assert_eq!(p.sutunlar[0].tur, SutunTuru::KayanNokta);
    }

    #[test]
    fn metne_yukselir() {
        let p = profil_uret_ile(&["n"], &[satir(&["1"]), satir(&["x"])]);
        assert_eq!(p.sutunlar[0].tur, SutunTuru::Metin);
    }

    #[test]
    fn mantiksal_tipi_cikarilir() {
        let p = profil_uret_ile(&["o"], &[satir(&["true"]), satir(&["FALSE"])]);
        assert_eq!(p.sutunlar[0].tur, SutunTuru::Mantiksal);
    }

    #[test]
    fn tarih_tipi_cikarilir() {
        let p = profil_uret_ile(&["t"], &[satir(&["2024-01-02"]), satir(&["03.04.2024"])]);
        assert_eq!(p.sutunlar[0].tur, SutunTuru::Tarih);
        assert_eq!(p.sutunlar[0].tarih_min.as_deref(), Some("2024-01-02"));
        assert_eq!(p.sutunlar[0].tarih_max.as_deref(), Some("2024-04-03"));
    }

    #[test]
    fn tarih_tutarsiz_degerler_sayilir() {
        let p = profil_uret_ile(
            &["t"],
            &[
                satir(&["2024-01-02"]),
                satir(&["2024-01-02"]),
                satir(&["2024-13-45"]),
            ],
        );
        assert_eq!(p.sutunlar[0].tarih_tutarsiz, 1);
    }

    #[test]
    fn tamamen_bos_sutun_tipi_bostur() {
        let p = profil_uret_ile(&["x"], &[satir(&[""]), satir(&[""])]);
        assert_eq!(p.sutunlar[0].tur, SutunTuru::Bos);
    }

    #[test]
    fn bos_deger_orani_hesaplanir() {
        let p = profil_uret_ile(
            &["x"],
            &[satir(&["1"]), satir(&[""]), satir(&[""]), satir(&["2"])],
        );
        let s = &p.sutunlar[0];
        assert_eq!(s.bos_sayisi, 2);
        assert_eq!(s.dolu_sayisi, 2);
        assert!((s.bos_oran - 0.5).abs() < 1e-9);
    }

    #[test]
    fn tamamen_bos_sutunda_oran_sifirdir() {
        let p = profil_uret_ile(&["x"], &[]);
        assert_eq!(p.sutunlar[0].bos_oran, 0.0);
    }

    #[test]
    fn sayisal_min_maks_ortalama() {
        let p = profil_uret_ile(
            &["n"],
            &[
                satir(&["10"]),
                satir(&["20"]),
                satir(&["60"]),
                satir(&["x"]),
            ],
        );
        let s = p.sutunlar[0].sayisal.as_ref().expect("sayisal");
        assert_eq!(s.adet, 3);
        assert_eq!(s.min, 10.0);
        assert_eq!(s.max, 60.0);
        assert!((s.ortalama - 30.0).abs() < 1e-9);
    }

    #[test]
    fn sayisal_olmayan_sutunda_istatistik_yoktur() {
        let p = profil_uret_ile(&["a"], &[satir(&["x"])]);
        assert!(p.sutunlar[0].sayisal.is_none());
    }

    #[test]
    fn en_sik_degerler_sikliga_gore_siralanir() {
        let p = profil_uret_ile(
            &["d"],
            &[
                satir(&["a"]),
                satir(&["a"]),
                satir(&["a"]),
                satir(&["b"]),
                satir(&["c"]),
            ],
        );
        let en_sik = &p.sutunlar[0].en_sik;
        assert_eq!(en_sik[0].deger, "a");
        assert_eq!(en_sik[0].adet, 3);
        assert_eq!(en_sik[1].deger, "b");
        assert_eq!(en_sik.len(), 3);
    }

    #[test]
    fn en_sik_deger_siniri_belirli() {
        let p = profil_uret_ile(&["d"], &[satir(&["a"]), satir(&["b"])]);
        assert_eq!(p.sutunlar[0].en_sik.len(), 2);
    }

    #[test]
    fn ayrik_deger_sayisi_sayilir() {
        let p = profil_uret_ile(&["d"], &[satir(&["a"]), satir(&["b"]), satir(&["a"])]);
        assert_eq!(p.sutunlar[0].farkli_deger_sayisi, 2);
        assert!(!p.sutunlar[0].farkli_deger_sinirda);
    }

    #[test]
    fn bicim_varyanti_kucuk_harf_duyarsiz() {
        let p = profil_uret_ile(&["d"], &[satir(&["Ali"]), satir(&["ali"]), satir(&["ALI"])]);
        assert_eq!(p.sutunlar[0].bicim_varyanti, 1);
    }

    #[test]
    fn ornek_satir_ve_uyari_sayisi_raporlanir() {
        let mut bozuk = satir(&["1"]);
        bozuk.uyari = Some("kapatilmamis tirnak".to_string());
        let p = profil_uret_ile(
            &["a", "b"],
            &[satir(&["1", "2"]), satir(&["3", "4"]), bozuk],
        );
        assert_eq!(p.ornek_satir_sayisi, 3);
        assert_eq!(p.uyarili_satir_sayisi, 1);
        assert_eq!(p.sutun_sayisi, 2);
    }

    #[test]
    fn sutun_sirasi_adla_eslesir() {
        let p = profil_uret_ile(&["ilk", "ikinci"], &[satir(&["1", "2"])]);
        assert_eq!(p.sutunlar[0].sira, 0);
        assert_eq!(p.sutunlar[1].ad, "ikinci");
    }

    #[test]
    fn sutunda_eksik_alan_bos_sayilir() {
        let p = profil_uret_ile(&["a", "b"], &[satir(&["1"])]);
        assert_eq!(p.sutunlar[0].dolu_sayisi, 1);
        assert_eq!(p.sutunlar[1].dolu_sayisi, 0);
    }

    #[test]
    fn atlanan_bos_satir_profilde_raporlanir() {
        let b = basliklar(&["a"]);
        let p = profil_uret(&bilgi(), &b, &[satir(&["1"])], 10, 4);
        assert_eq!(p.satir_sayisi, 10);
        assert_eq!(p.atlanan_bos_satir, 4);
    }

    #[test]
    fn profil_json_serilestirilebilir() {
        let p = profil_uret_ile(&["n"], &[satir(&["1"])]);
        let metin = serde_json::to_string(&p).expect("json");
        assert!(metin.contains("\"tur\":\"tam_sayi\""));
        assert!(metin.contains("\"bos_oran\""));
    }
}
