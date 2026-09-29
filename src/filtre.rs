//! Sütun karşılaştırması tabanlı filtreleme.
//!
//! Filtre dili kasıtlı olarak **kapalı ve dardır**: dört operatör
//! (`=`, `>`, `<`, `contains`), mantıksal bileşim yalnızca VE'dir, dosya, ağ
//! ve süreç işlevi yoktur (rapor b10 tehdit modeli).
//!
//! Sonuç **satır numarası listesi** olarak döner; satır verisi kopyalanmaz
//! (rapor b07, madde 5). Böylece filtre uygulanmış küme bellekte yalnızca
//! `8 × satır sayısı` bayt tutar.
//!
//! Karşılaştırma kuralı: hücre ve filtre değeri **ikisi de** sayıya
//! çevrilebiliyorsa sayısal karşılaştırma yapılır, aksi hâlde bayt sırasıyla
//! sözel karşılaştırma yapılır. Bu, sütun tipi çıkarımından bağımsız ve
//! öngörülebilir bir davranıştır.

use std::fmt;

use serde::Serialize;

use crate::hata::Hata;
use crate::kaynak::Satir;
use crate::tip::sayiya;

/// Desteklenen karşılaştırma operatörleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    /// Tam eşitlik.
    Esitlik,
    /// Büyüktür.
    Buyuk,
    /// Küçüktür.
    Kucuk,
    /// Aranan parçayı içerir (büyük/küçük harf duyarsız).
    Icerir,
}

impl Operator {
    /// Sembol gösterimi.
    pub fn simge(self) -> &'static str {
        match self {
            Operator::Esitlik => "=",
            Operator::Buyuk => ">",
            Operator::Kucuk => "<",
            Operator::Icerir => "contains",
        }
    }

    /// Metin gösterimi (kullanıcı hata mesajlarında).
    pub fn ad(self) -> &'static str {
        match self {
            Operator::Esitlik => "esitlik",
            Operator::Buyuk => "buyuk",
            Operator::Kucuk => "kucuk",
            Operator::Icerir => "icerir",
        }
    }

    /// Simge ya da ad biçiminden operatör çözer.
    pub fn ayristir(metin: &str) -> Option<Operator> {
        match metin.trim().to_ascii_lowercase().as_str() {
            "=" | "==" | "eq" => Some(Operator::Esitlik),
            ">" | "gt" => Some(Operator::Buyuk),
            "<" | "lt" => Some(Operator::Kucuk),
            "contains" | "icerir" | "~" | "like" => Some(Operator::Icerir),
            _ => None,
        }
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.simge())
    }
}

/// Derlenmiş tek bir filtre koşulu.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Filtre {
    /// Sütunun sırası.
    pub sutun: usize,
    /// Sütunun adı.
    pub sutun_adi: String,
    /// Karşılaştırma operatörü.
    pub operator: Operator,
    /// Karşılaştırılacak değer (metin olarak).
    pub deger: String,
    /// Değerin sayısal karşılığı varsa burada tutulur.
    pub deger_sayisal: Option<f64>,
}

impl Filtre {
    /// Hücre değerinin koşulu sağlayıp sağlamadığını söyler.
    pub fn eslesir(&self, alan: &str) -> bool {
        match self.operator {
            Operator::Icerir => alan.to_lowercase().contains(&self.deger.to_lowercase()),
            _ => match (sayiya(alan), self.deger_sayisal) {
                (Some(h), Some(d)) => match self.operator {
                    Operator::Esitlik => h == d,
                    Operator::Buyuk => h > d,
                    Operator::Kucuk => h < d,
                    Operator::Icerir => unreachable!("Icerir yukarida dondu"),
                },
                _ => match self.operator {
                    Operator::Esitlik => alan == self.deger,
                    Operator::Buyuk => alan > self.deger.as_str(),
                    Operator::Kucuk => alan < self.deger.as_str(),
                    Operator::Icerir => unreachable!("Icerir yukarida dondu"),
                },
            },
        }
    }
}

/// Birden çok koşulun VE bileşimi.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FiltreKumesi {
    /// Bileşik edilecek koşullar.
    pub kosullar: Vec<Filtre>,
}

impl FiltreKumesi {
    /// Boş küme, her satırı kabul eder.
    pub fn bos() -> Self {
        FiltreKumesi {
            kosullar: Vec::new(),
        }
    }

    /// Verilen ifadeleri sütun adlarına göre derler.
    ///
    /// Her ifade `<sütun> <operatör> <değer>` biçimindedir. Sütun adı boşluk
    /// içeriyorsa infix biçim (`ad="ilk okul"`) kullanılmalıdır.
    pub fn derle(ifadeler: &[String], basliklar: &[String]) -> Result<FiltreKumesi, Hata> {
        let mut kosullar = Vec::with_capacity(ifadeler.len());
        for ifade in ifadeler {
            kosullar.push(derle_bir(ifade, basliklar)?);
        }
        Ok(FiltreKumesi { kosullar })
    }

    /// Kümenin boş olup olmadığını söyler.
    pub fn bos_mu(&self) -> bool {
        self.kosullar.is_empty()
    }

    /// Bir satır tüm koşulları sağlıyorsa `true`.
    pub fn eslesir(&self, satir: &Satir) -> bool {
        self.kosullar.iter().all(|k| {
            satir
                .alanlar
                .get(k.sutun)
                .map(|a| k.eslesir(a))
                .unwrap_or(false)
        })
    }

    /// Filtreyi uygular ve eşleşen satır numaralarını döndürür.
    ///
    /// Satır verisi kopyalanmaz; sonuç yalnızca indeks referanslarıdır.
    pub fn uygula<F>(&self, satir_sayisi: u64, mut oku: F) -> Vec<u64>
    where
        F: FnMut(u64) -> Satir,
    {
        let mut sonuc = Vec::new();
        for no in 0..satir_sayisi {
            let satir = oku(no);
            if self.eslesir(&satir) {
                sonuc.push(no);
            }
        }
        sonuc
    }
}

/// Tek bir filtre ifadesini ayrıştırır ve derler.
fn derle_bir(ifade: &str, basliklar: &[String]) -> Result<Filtre, Hata> {
    let bozuk = |sebep: &'static str| Hata::BozukFiltre {
        ifade: ifade.to_string(),
        sebep,
    };

    let ilk_bolme = ifade
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, _)| i);
    let ilk_token = match ilk_bolme {
        Some(i) => &ifade[..i],
        None => ifade,
    };

    // Infix bicim: operator ilk token'in icinde bitisik ("sira>5", "ad=ali",
    // "ad=a b"). Sutun adi iki noktali virgul iceriyorsa infix bicim
    // kullanilamaz; sozel bicim kullanilmalidir.
    let infix = ilk_token
        .char_indices()
        .find(|(_, c)| matches!(c, '=' | '>' | '<'))
        .filter(|(i, _)| *i > 0);
    let (sutun_adi, operator, deger) = match infix {
        Some((konum, simge)) => {
            let operator = match simge {
                '=' => Operator::Esitlik,
                '>' => Operator::Buyuk,
                _ => Operator::Kucuk,
            };
            let sutun = &ilk_token[..konum];
            let kalan = &ilk_token[konum + simge.len_utf8()..];
            let deger = format!(
                "{kalan}{}",
                ifade[ilk_bolme.unwrap_or(ilk_token.len())..].trim()
            );
            (sutun.to_string(), operator, deger.trim().to_string())
        }
        None => {
            let kalan = ifade[ilk_bolme.ok_or_else(|| bozuk("sutun adi eksik"))?..].trim_start();
            let op_token_son = kalan
                .char_indices()
                .find(|(_, c)| c.is_whitespace())
                .map(|(i, _)| i)
                .unwrap_or(kalan.len());
            let op_token = &kalan[..op_token_son];
            let operator =
                Operator::ayristir(op_token).ok_or_else(|| bozuk("operator taninmadi"))?;
            let deger = kalan[op_token_son..].trim();
            (ilk_token.to_string(), operator, deger.to_string())
        }
    };

    let sutun_adi = sutun_adi.trim();
    if sutun_adi.is_empty() {
        return Err(bozuk("sutun adi eksik"));
    }
    let sutun = sutun_no(sutun_adi, basliklar)?;
    let deger = tirnaklari_soy(&deger);

    Ok(Filtre {
        sutun,
        sutun_adi: basliklar[sutun].clone(),
        operator,
        deger_sayisal: sayiya(&deger),
        deger,
    })
}

/// Değerin çevreleyen tırnaklarını (varsa) çıkarır.
fn tirnaklari_soy(deger: &str) -> String {
    let bayt = deger.as_bytes();
    let tirnakli =
        bayt.len() >= 2 && (bayt[0] == b'"' || bayt[0] == b'\'') && bayt[bayt.len() - 1] == bayt[0];
    if tirnakli {
        deger[1..deger.len() - 1].to_string()
    } else {
        deger.to_string()
    }
}

/// Sütun adını (ya da sıra numarasını) çözer.
fn sutun_no(ad: &str, basliklar: &[String]) -> Result<usize, Hata> {
    if let Some(i) = basliklar.iter().position(|b| b == ad) {
        return Ok(i);
    }
    let kucuk = ad.to_lowercase();
    if let Some(i) = basliklar.iter().position(|b| b.to_lowercase() == kucuk) {
        return Ok(i);
    }
    if let Ok(i) = ad.parse::<usize>() {
        if i < basliklar.len() {
            return Ok(i);
        }
    }
    Err(Hata::SutunYok {
        sutun: ad.to_string(),
        sutun_sayisi: basliklar.len(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn basliklar() -> Vec<String> {
        ["ad", "yas", "not"]
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    fn derle(ifade: &str) -> Filtre {
        FiltreKumesi::derle(&[ifade.to_string()], &basliklar())
            .expect("derle")
            .kosullar
            .remove(0)
    }

    fn satir(alanlar: &[&str]) -> Satir {
        Satir {
            no: 0,
            alanlar: alanlar.iter().map(|a| (*a).to_string()).collect(),
            json: None,
            uyari: None,
        }
    }

    #[test]
    fn operator_sembolleri_cozulur() {
        assert_eq!(Operator::ayristir("="), Some(Operator::Esitlik));
        assert_eq!(Operator::ayristir(">"), Some(Operator::Buyuk));
        assert_eq!(Operator::ayristir("<"), Some(Operator::Kucuk));
        assert_eq!(Operator::ayristir("contains"), Some(Operator::Icerir));
        assert_eq!(Operator::ayristir("YOK"), None);
    }

    #[test]
    fn operator_gosterimleri_ve_display() {
        assert_eq!(Operator::Esitlik.simge(), "=");
        assert_eq!(Operator::Icerir.ad(), "icerir");
        assert_eq!(Operator::Buyuk.to_string(), ">");
    }

    #[test]
    fn infix_esitlik_ifadesi_cozulur() {
        let f = derle("ad=ali");
        assert_eq!(f.sutun, 0);
        assert_eq!(f.sutun_adi, "ad");
        assert_eq!(f.operator, Operator::Esitlik);
        assert_eq!(f.deger, "ali");
    }

    #[test]
    fn sozel_buyuk_ifadesi_cozulur() {
        let f = derle("yas > 30");
        assert_eq!(f.sutun, 1);
        assert_eq!(f.operator, Operator::Buyuk);
        assert_eq!(f.deger_sayisal, Some(30.0));
    }

    #[test]
    fn tirnakli_deger_sovulur() {
        let f = derle(r#"not = "ilk okul""#);
        assert_eq!(f.deger, "ilk okul");
    }

    #[test]
    fn sozel_icerir_ifadesi_cozulur() {
        let f = derle("ad contains ker");
        assert_eq!(f.operator, Operator::Icerir);
        assert_eq!(f.deger, "ker");
    }

    #[test]
    fn sutun_adi_indeks_olarak_cozulur() {
        let f = derle("1>10");
        assert_eq!(f.sutun, 1);
    }

    #[test]
    fn sutun_adi_buyuk_kucuk_harf_duyarsiz_cozulur() {
        let f = derle("YAS>10");
        assert_eq!(f.sutun, 1);
    }

    #[test]
    fn bilinmeyen_sutun_hata_verir() {
        let sonuc = FiltreKumesi::derle(&["olmayan=1".to_string()], &basliklar());
        assert!(matches!(sonuc, Err(Hata::SutunYok { .. })));
    }

    #[test]
    fn bilinmeyen_operator_hata_verir() {
        let sonuc = FiltreKumesi::derle(&["yas != 5".to_string()], &basliklar());
        assert!(matches!(sonuc, Err(Hata::BozukFiltre { .. })));
    }

    #[test]
    fn operator_eksik_ifade_hata_verir() {
        let sonuc = FiltreKumesi::derle(&["yas".to_string()], &basliklar());
        assert!(matches!(sonuc, Err(Hata::BozukFiltre { .. })));
    }

    #[test]
    fn esitlik_sayisal_karsilastirma_yapar() {
        let f = derle("yas=30");
        assert!(f.eslesir("30"));
        assert!(f.eslesir("30.0"));
        assert!(!f.eslesir("31"));
    }

    #[test]
    fn esitlik_sayisal_deger_degilse_sozel_karsilastirma_yapar() {
        let f = derle("ad=ali");
        assert!(f.eslesir("ali"));
        assert!(!f.eslesir("ALI"));
    }

    #[test]
    fn buyuk_sayisal_karsilastirma_yapar() {
        let f = derle("yas>30");
        assert!(!f.eslesir("30"));
        assert!(f.eslesir("31"));
        assert!(f.eslesir("100"));
    }

    #[test]
    fn buyuk_sozel_karsilastirma_yapar() {
        let f = derle("ad>b");
        assert!(f.eslesir("c"));
        assert!(!f.eslesir("a"));
        assert!(!f.eslesir("b"));
    }

    #[test]
    fn kucuk_sozel_karsilastirma_yapar() {
        let f = derle("ad<b");
        assert!(f.eslesir("a"));
        assert!(!f.eslesir("b"));
        assert!(!f.eslesir("c"));
    }

    #[test]
    fn icerir_buyuk_kucuk_harf_duyarsizdir() {
        let f = derle("ad contains ALI");
        assert!(f.eslesir("ali"));
        assert!(f.eslesir("Ali_veli"));
        assert!(!f.eslesir("ayse"));
    }

    #[test]
    fn bos_hucre_durumlarinin_sinirlari_belgirdir() {
        let f = derle("yas>0");
        // Bos hucrenin sayisal karsilastirmasi yapilamaz; sozel karsilastirmaya
        // duser ve "" < "0" oldugu icin eslesmez.
        assert!(!f.eslesir(""));
        let f2 = derle("yas<0");
        assert!(f2.eslesir(""));
    }

    #[test]
    fn bilesim_tum_kosullari_ister() {
        let kume = FiltreKumesi::derle(
            &["yas>30".to_string(), "ad contains a".to_string()],
            &basliklar(),
        )
        .expect("derle");
        assert_eq!(kume.kosullar.len(), 2);
        assert!(kume.eslesir(&satir(&["ali", "31", "x"])));
        assert!(!kume.eslesir(&satir(&["ali", "29", "x"])));
        assert!(!kume.eslesir(&satir(&["veli", "31", "x"])));
    }

    #[test]
    fn bos_kume_her_satiri_kabul_eder() {
        let kume = FiltreKumesi::bos();
        assert!(kume.bos_mu());
        assert!(kume.eslesir(&satir(&["x"])));
    }

    #[test]
    fn uygula_satir_numarasi_dondurur() {
        let kume = FiltreKumesi::derle(&["yas>30".to_string()], &basliklar()).expect("derle");
        let veri = [
            satir(&["a", "29", ""]),
            satir(&["b", "31", ""]),
            satir(&["c", "40", ""]),
        ];
        let sonuc = kume.uygula(3, |no| veri[no as usize].clone());
        assert_eq!(sonuc, vec![1, 2]);
    }

    #[test]
    fn eksik_sutunda_kosul_eslesmez() {
        let kume = FiltreKumesi::derle(&["not=abc".to_string()], &basliklar()).expect("derle");
        let kisa = Satir {
            no: 0,
            alanlar: vec!["a".to_string(), "1".to_string()],
            json: None,
            uyari: None,
        };
        assert!(!kume.eslesir(&kisa));
    }

    #[test]
    fn filtre_json_serilestirmesi_alani_adlarini_icerir() {
        let kume = FiltreKumesi::derle(&["yas>30".to_string()], &basliklar()).expect("derle");
        let metin = serde_json::to_string(&kume).unwrap();
        assert!(metin.contains("\"sutun_adi\":\"yas\""));
        assert!(metin.contains("\"buyuk\""));
    }
}
