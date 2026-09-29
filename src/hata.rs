//! Hata tipleri ve bunların `Display` uygulamaları.
//!
//! Sorumluluğu, tüm modüllerin ortak kullandığı tek hata sınıfını tanımlamaktır.
//! `thiserror` bağımlılığı yasak olduğu için (`WORKER_CONTRACT.md` § 4.3) `Display`
//! ve `Error` uygulamaları elle yazılmıştır.
//!
//! Tüm hatalar kullanıcı girdisinden veya dosya sisteminden türetilir; hiçbir
//! hata `panic!` ile karşılanmaz. Yalnızca programcı hatası (invariant bozulması)
//! `panic!` üretebilir ve kodda böyle bir yol yoktur.

use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

/// DataLens'in tüm modüllerinde döndürülen hata tipi.
///
/// Dosya sistemi hataları ve kullanıcı girdisi kaynaklı hatalar ayrı türlerde
/// tutulur, böylece çağıran taraf kök nedeni ayırtabilir.
#[derive(Debug)]
#[non_exhaustive]
pub enum Hata {
    /// Bir dosya üzerinde G/Ç işlemi başarısız oldu.
    Io {
        /// Yapılmaya çalışılan işlemin kısa adı (ör. `"acma"`, `"indeksleme"`).
        islem: &'static str,
        /// İşlemin uygulandığı yol.
        yol: PathBuf,
        /// Altta yatan `std::io` hatası.
        kaynak: io::Error,
    },
    /// Verilen yol bir dosya değil (dizin, soket vb.).
    DosyaDegil {
        /// Verilen yol.
        yol: PathBuf,
    },
    /// Dosya adı uzantısından ve içerikten biçim belirlenemedi.
    BilinmeyenBicim {
        /// İncelenen yol.
        yol: PathBuf,
    },
    /// `--ayrac` ile verilen değer tek bir bayt değil.
    GecersizAyrac {
        /// Kullanıcının verdiği değer.
        deger: String,
    },
    /// Filtre ifadesi çözümlenemedi.
    BozukFiltre {
        /// Kullanıcının verdiği ifade.
        ifade: String,
        /// Ayrıştırma sırasında bulunan sorunun açıklaması.
        sebep: &'static str,
    },
    /// Filtrede adı geçen sütun tabloda yok.
    SutunYok {
        /// Aranan sütun adı.
        sutun: String,
        /// Tablodaki sütun sayısı.
        sutun_sayisi: usize,
    },
    /// Satır aralığı geçersiz (dosya sonunu aşıyor).
    GecersizAralik {
        /// İstenen başlangıç satırı.
        baslangic: u64,
        /// İstenen satır sayısı.
        adet: u64,
        /// Dosyadaki gerçek satır sayısı.
        satir_sayisi: u64,
    },
    /// Dışa aktarım hedefi yazılamadı.
    CiktiHatasi {
        /// Hedef yol.
        yol: PathBuf,
        /// Altta yatan `std::io` hatası.
        kaynak: io::Error,
    },
    /// Sütun adı çözümlenemedi ya da indekslenemedi (programcı hatası değil,
    /// ancak dosya bozulmuş olabilir).
    SutunCozulemedi {
        /// Çözümlenemek istenen ad.
        ad: String,
    },
}

impl fmt::Display for Hata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Hata::Io { islem, yol, kaynak } => {
                write!(f, "{islem} basarisiz: {} ({})", yol.display(), kaynak)
            }
            Hata::DosyaDegil { yol } => {
                write!(f, "bu yol bir dosya degil: {}", yol.display())
            }
            Hata::BilinmeyenBicim { yol } => write!(
                f,
                "dosya bicimi belirlenemedi: {} (uzanti ve icerik yetersiz; --bicim ile belirtin)",
                yol.display()
            ),
            Hata::GecersizAyrac { deger } => write!(
                f,
                "gecersiz ayrac: {deger:?} (tek bir bayt olmali; ornek: , ; \\t |)"
            ),
            Hata::BozukFiltre { ifade, sebep } => {
                write!(f, "filtre ifadesi cozulemedi: {ifade:?} ({sebep})")
            }
            Hata::SutunYok {
                sutun,
                sutun_sayisi,
            } => write!(
                f,
                "sutun bulunamadi: {sutun:?} (tablo {sutun_sayisi} sutun iceriyor)"
            ),
            Hata::GecersizAralik {
                baslangic,
                adet,
                satir_sayisi,
            } => write!(
                f,
                "gecersiz satir araligi: {baslangic}..{} (dosyada {satir_sayisi} satir var)",
                baslangic.saturating_add(*adet)
            ),
            Hata::CiktiHatasi { yol, kaynak } => {
                write!(f, "cikti yazilamadi: {} ({})", yol.display(), kaynak)
            }
            Hata::SutunCozulemedi { ad } => {
                write!(f, "sutun cozulemedi: {ad}")
            }
        }
    }
}

impl Error for Hata {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Hata::Io { kaynak, .. } | Hata::CiktiHatasi { kaynak, .. } => Some(kaynak),
            _ => None,
        }
    }
}

/// `Hata::Io` üreten yardımcı: yol taşımayı çağıran taraftan gizler.
pub(crate) fn io_hata(islem: &'static str, yol: &std::path::Path, kaynak: io::Error) -> Hata {
    Hata::Io {
        islem,
        yol: yol.to_path_buf(),
        kaynak,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn goster(h: &Hata) -> String {
        h.to_string()
    }

    #[test]
    fn io_hatasi_yolu_ve_kaynagi_gosterir() {
        let h = io_hata(
            "acma",
            std::path::Path::new("veri.csv"),
            io::Error::new(io::ErrorKind::NotFound, "yok"),
        );
        let metin = goster(&h);
        assert!(metin.contains("acma"));
        assert!(metin.contains("veri.csv"));
        assert!(metin.contains("yok"));
        assert!(h.source().is_some());
    }

    #[test]
    fn gecersiz_aralik_mesaji_sinirlari_yazar() {
        let h = Hata::GecersizAralik {
            baslangic: 10,
            adet: 5,
            satir_sayisi: 12,
        };
        let metin = goster(&h);
        assert!(metin.contains("10..15"));
        assert!(metin.contains("12"));
        assert!(h.source().is_none());
    }

    #[test]
    fn gecersiz_ayrac_mesaji_beklenenleri_ogrenir() {
        let h = Hata::GecersizAyrac {
            deger: "||".to_string(),
        };
        assert!(goster(&h).contains("tek bir bayt"));
    }

    #[test]
    fn bozuk_filtre_mesaji_ifadeyi_skrar() {
        let h = Hata::BozukFiltre {
            ifade: "sira>>".to_string(),
            sebep: "operator taninmadi",
        };
        let metin = goster(&h);
        assert!(metin.contains("sira>>"));
        assert!(metin.contains("operator taninmadi"));
    }

    #[test]
    fn sutun_yok_mesaji_sutun_sayisini_yazar() {
        let h = Hata::SutunYok {
            sutun: "yok".to_string(),
            sutun_sayisi: 3,
        };
        let metin = goster(&h);
        assert!(metin.contains("yok"));
        assert!(metin.contains("3 sutun"));
    }

    #[test]
    fn bilinmeyen_bicim_hatasi_bicim_ipucu_verir() {
        let h = Hata::BilinmeyenBicim {
            yol: PathBuf::from("a.dat"),
        };
        assert!(goster(&h).contains("--bicim"));
    }

    #[test]
    fn cikti_hatasi_kaynagi_yoksa_kaynak_yoktur() {
        let h = Hata::CiktiHatasi {
            yol: PathBuf::from("out.csv"),
            kaynak: io::Error::other("dolu"),
        };
        assert!(goster(&h).contains("out.csv"));
        assert!(h.source().is_some());
    }

    #[test]
    fn dosya_degil_mesaji_yolu_gosterir() {
        let h = Hata::DosyaDegil {
            yol: PathBuf::from("klasor"),
        };
        assert!(goster(&h).contains("klasor"));
    }

    #[test]
    fn sutun_cozulemedi_mesaji_ogrenilir() {
        let h = Hata::SutunCozulemedi {
            ad: "x".to_string(),
        };
        assert!(goster(&h).contains("x"));
    }
}
