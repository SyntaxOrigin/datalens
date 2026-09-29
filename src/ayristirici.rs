//! Bayt düzeyinde kayıt sınırlama ve alan ayrıştırma.
//!
//! Sorumluluğu iki işlevdir:
//!
//! 1. **Sınırlama** ([`kayit_tara`]): tampon içinde bir kaydın nerede bittiğini
//!    bulur. Tırnak içindeki ayraçları ve satır sonlarını yok sayar; JSON'da
//!    dizge içindeki `\n` karakterini de yok sayar. Bu fonksiyon dosyanın tamamını
//!    görmeden çalışır, tampon bitirse [`Tarama::Devam`] döner.
//! 2. **Ayrıştırma** ([`ayristir`]): sınırlanmış bir kaydın baytlarından alan
//!    listesini üretir ve RFC 4180 dışı durumları uyarı olarak bildirir.
//!
//! Ne **değil**: bu modül dosya sistemine dokunmaz ve satır indeksi tutmaz.

use crate::bicim::Bicim;

/// Kayıt sınırlayıcısının döndürdüğü sonuç.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tarama {
    /// Kayıt bulundu. İçerik `baslangic..son` (tampon içi, `son` hariç), bir
    /// sonraki kayıt `sonraki` konumundan başlar.
    Tamam {
        /// Kaydın içeriğinin başladığı tampon konumu.
        baslangic: usize,
        /// Kaydın içeriğinin bittiği tampon konumu (satır sonu hariç).
        son: usize,
        /// Bir sonraki kaydın başladığı tampon konumu.
        sonraki: usize,
    },
    /// Tampon kaydı tamamlamaya yetmedi; tampon büyütülüp yeniden denenmelidir.
    Devam,
    /// JSON dizi kökünde dizinin kapanışı (`]`) görüldü; indeksleme bitti.
    DiziBitti,
}

/// Kayıt sınırlayıcısının ihtiyaç duyduğu çözümleme bilgisi.
#[derive(Debug, Clone, Copy)]
pub struct KayitCozum {
    /// Dosyanın biçimi.
    pub bicim: Bicim,
    /// CSV/TSV ayracı; JSONL'de 0.
    pub ayrac: u8,
    /// JSONL dosyasının kökü JSON dizi ise `true`.
    pub dizi_kok: bool,
}

/// Bir kaydın ayrıştırılmış hâli.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AyristirilmisKayit {
    /// Kaydın alanları. CSV/TSV'de ayrıştırılan ham alan sayısıdır; başlık
    /// sayısıyla eşleşmeyebilir (bu durum uyarı olarak bildirilir).
    pub alanlar: Vec<String>,
    /// Alanları doldururken tutulan, tekrar etmeyen uyarılar.
    pub uyarilar: Vec<String>,
    /// JSONL kaynağında ayrıştırılan özgün değer; CSV/TSV'de `None`.
    ///
    /// Dışa aktarımda JSONL çıktısı bu değerden üretilir; böylece sayı, mantıksal
    /// ve iç içe nesne tipleri metne indirgenmez.
    pub json: Option<serde_json::Value>,
}

impl AyristirilmisKayit {
    /// Uyarıları tek satırda birleştirir.
    pub fn uyari_metni(&self) -> Option<String> {
        if self.uyarilar.is_empty() {
            None
        } else {
            Some(self.uyarilar.join("; "))
        }
    }
}

/// CSV/TSV kaydın bittiği konumu bulur.
///
/// Kayıt sınırı, tırnak içinde olmayan satır sonlarıyla belirlenir; ayraç
/// yalnızca "tırnak bir alanın başında mı?" sorusunu yanıtlamak için gerekir.
/// Bu ayrım önemlidir: tırnaksız alanın ortasındaki bir tırnak (ör.
/// `2,ayse,de"diye`) **veridir**, alan açılışı değildir. Alan ayrıştırıcısıyla
/// aynı kural uygulanmazsa indeks ile ayrıştırıcı farklı kayıt sayıları
/// üretir ve satırlar sessizce birleşir.
fn csv_tara(tampon: &[u8], baslangic: usize, ayrac: u8, son: bool) -> Tarama {
    let mut i = baslangic;
    let mut tirnakli = false;
    let mut alan_basi = true;
    while i < tampon.len() {
        let b = tampon[i];
        if b == b'"' {
            if !tirnakli {
                if alan_basi {
                    tirnakli = true;
                }
                alan_basi = false;
                i += 1;
                continue;
            }
            if i + 1 < tampon.len() {
                if tampon[i + 1] == b'"' {
                    i += 2;
                    continue;
                }
                tirnakli = false;
                alan_basi = false;
                i += 1;
                continue;
            }
            // Tampon bitti: sonraki bayt kacisli tirnak mi ayirt edilemez.
            if !son {
                return Tarama::Devam;
            }
            tirnakli = false;
            alan_basi = false;
            i += 1;
            continue;
        }
        if !tirnakli {
            if b == b'\n' {
                return Tarama::Tamam {
                    baslangic,
                    son: i,
                    sonraki: i + 1,
                };
            }
            if b == b'\r' {
                if i + 1 >= tampon.len() && !son {
                    return Tarama::Devam;
                }
                let sonraki = if i + 1 < tampon.len() && tampon[i + 1] == b'\n' {
                    i + 2
                } else {
                    i + 1
                };
                return Tarama::Tamam {
                    baslangic,
                    son: i,
                    sonraki,
                };
            }
            alan_basi = b == ayrac;
        }
        i += 1;
    }
    if son {
        Tarama::Tamam {
            baslangic,
            son: tampon.len(),
            sonraki: tampon.len(),
        }
    } else {
        Tarama::Devam
    }
}

/// JSON Lines kaydının bittiği konumu bulur (dizge içi `\n` yok sayılır).
fn json_satir_tara(tampon: &[u8], baslangic: usize, son: bool) -> Tarama {
    let mut i = baslangic;
    let mut tirnakta = false;
    let mut kacis = false;
    while i < tampon.len() {
        let b = tampon[i];
        if kacis {
            kacis = false;
            i += 1;
            continue;
        }
        if tirnakta {
            match b {
                b'\\' => kacis = true,
                b'"' => tirnakta = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                tirnakta = true;
                i += 1;
            }
            b'\n' => {
                let mut son_i = i;
                if son_i > baslangic && tampon[son_i - 1] == b'\r' {
                    son_i -= 1;
                }
                return Tarama::Tamam {
                    baslangic,
                    son: son_i,
                    sonraki: i + 1,
                };
            }
            _ => i += 1,
        }
    }
    if son {
        let mut son_i = tampon.len();
        if son_i > baslangic && tampon[son_i - 1] == b'\r' {
            son_i -= 1;
        }
        Tarama::Tamam {
            baslangic,
            son: son_i,
            sonraki: tampon.len(),
        }
    } else {
        Tarama::Devam
    }
}

/// JSON dizi kökünde bir elemanın bittiği konumu bulur.
///
/// `derinlik == 0` iken görülen `,` elemanı bitirir, `]` ise dizinin sonunu
/// işaretler ve [`Tarama::DiziBitti`] döner.
fn json_dizi_tara(tampon: &[u8], baslangic: usize, son: bool) -> Tarama {
    let mut i = baslangic;
    let mut tirnakta = false;
    let mut kacis = false;
    let mut derinlik: i32 = 0;
    while i < tampon.len() {
        let b = tampon[i];
        if kacis {
            kacis = false;
            i += 1;
            continue;
        }
        if tirnakta {
            match b {
                b'\\' => kacis = true,
                b'"' => tirnakta = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                tirnakta = true;
                i += 1;
            }
            b'{' | b'[' => {
                derinlik += 1;
                i += 1;
            }
            b'}' | b']' => {
                if derinlik == 0 {
                    // Dizi kapandi. Son eleman virgulle degil `]` ile biter;
                    // onu bir kayit olarak dondur, sonraki cagri DiziBitti verir.
                    // Bos eleman (or. `[ ]` ya da sondaki virgul) kayit degildir.
                    return if i == baslangic {
                        Tarama::DiziBitti
                    } else {
                        Tarama::Tamam {
                            baslangic,
                            son: i,
                            sonraki: i,
                        }
                    };
                }
                derinlik -= 1;
                i += 1;
            }
            b',' if derinlik == 0 => {
                return Tarama::Tamam {
                    baslangic,
                    son: i,
                    sonraki: i + 1,
                }
            }
            _ => i += 1,
        }
    }
    if son {
        Tarama::Tamam {
            baslangic,
            son: tampon.len(),
            sonraki: tampon.len(),
        }
    } else {
        Tarama::Devam
    }
}

/// Tampon içinde `baslangic` konumundan itibaren bir kaydın sınırlarını bulur.
///
/// `son` parametresi tamponun dosyanın sonuna kadar uzandığını belirtir; yalnızca
/// bu durumda tampon bitişi "kayıt bitti" sayılabilir. Aksi hâlde
/// [`Tarama::Devam`] döner ve çağıran tamponu büyütmelidir.
pub fn kayit_tara(tampon: &[u8], baslangic: usize, cozum: &KayitCozum, son: bool) -> Tarama {
    match cozum.bicim {
        Bicim::Csv | Bicim::Tsv => csv_tara(tampon, baslangic, cozum.ayrac, son),
        Bicim::Jsonl => {
            if cozum.dizi_kok {
                json_dizi_tara(tampon, baslangic, son)
            } else {
                json_satir_tara(tampon, baslangic, son)
            }
        }
    }
}

/// Uyarı listesine yalnızca yeni bir mesaj eklerse ekler.
fn uyari_ekle(uyarilar: &mut Vec<String>, mesaj: &str) {
    if !uyarilar.iter().any(|m| m == mesaj) {
        uyarilar.push(mesaj.to_string());
    }
}

/// Bayt dizisini metne çevirir; geçersiz UTF-8 varsa boş dize döner ve uyarır.
///
/// Geçersiz baytlar **sessizce değiştirilmez**; alan boşaltılır ve uyarı
/// üretilir. Veri kaybı sessiz olmaktan çıkar, görünür olur.
fn metne(ham: &[u8], uyarilar: &mut Vec<String>) -> String {
    match std::str::from_utf8(ham) {
        Ok(s) => s.to_string(),
        Err(_) => {
            uyari_ekle(uyarilar, "gecersiz UTF-8; ilgili alan bosaltildi");
            String::new()
        }
    }
}

/// CSV/TSV alanlarını ayrıştırır.
fn csv_alanlar(baytlar: &[u8], ayrac: u8, uyarilar: &mut Vec<String>) -> Vec<String> {
    let mut alanlar: Vec<String> = Vec::new();
    let mut i = 0usize;
    loop {
        if i >= baytlar.len() {
            alanlar.push(String::new());
            break;
        }
        if baytlar[i] == b'"' {
            i += 1;
            let mut ham: Vec<u8> = Vec::new();
            let mut kapandi = false;
            while i < baytlar.len() {
                if baytlar[i] == b'"' {
                    if i + 1 < baytlar.len() && baytlar[i + 1] == b'"' {
                        ham.push(b'"');
                        i += 2;
                        continue;
                    }
                    kapandi = true;
                    i += 1;
                    break;
                }
                ham.push(baytlar[i]);
                i += 1;
            }
            if !kapandi {
                uyari_ekle(uyarilar, "kapatilmamis tirnak");
            }
            if i < baytlar.len() && baytlar[i] != ayrac {
                uyari_ekle(uyarilar, "tirnak kapandiktan sonra veri var");
                while i < baytlar.len() && baytlar[i] != ayrac {
                    ham.push(baytlar[i]);
                    i += 1;
                }
            }
            alanlar.push(metne(&ham, uyarilar));
        } else {
            let baslangic = i;
            let mut tirnak_var = false;
            while i < baytlar.len() && baytlar[i] != ayrac {
                if baytlar[i] == b'"' {
                    tirnak_var = true;
                }
                i += 1;
            }
            if tirnak_var {
                uyari_ekle(uyarilar, "tirnaksiz alan icinde tirnak var");
            }
            alanlar.push(metne(&baytlar[baslangic..i], uyarilar));
        }
        if i < baytlar.len() && baytlar[i] == ayrac {
            i += 1;
            continue;
        }
        break;
    }
    alanlar
}

/// Sınırlanmış bir kaydın baytlarından alan listesini üretir.
///
/// Hatalı kayıtlar iptal edilmez: alan sayısı korunur, geçersiz alanlar boş
/// bırakılır ve `uyarilar` doldurulur. Bu, raporun "kullanıcı hatalı satırı
/// sessizce kaybetmez" kararının doğrudan uygulamasıdır.
pub fn ayristir(baytlar: &[u8], bicim: Bicim, ayrac: u8) -> AyristirilmisKayit {
    let mut uyarilar: Vec<String> = Vec::new();
    match bicim {
        Bicim::Csv | Bicim::Tsv => {
            let alanlar = csv_alanlar(baytlar, ayrac, &mut uyarilar);
            AyristirilmisKayit {
                alanlar,
                uyarilar,
                json: None,
            }
        }
        Bicim::Jsonl => match std::str::from_utf8(baytlar) {
            Ok(metin) => match serde_json::from_str::<serde_json::Value>(metin) {
                Ok(deger) => {
                    let alanlar = match &deger {
                        serde_json::Value::Object(harita) => {
                            harita.values().map(deger_metni).collect()
                        }
                        diger => vec![deger_metni(diger)],
                    };
                    AyristirilmisKayit {
                        alanlar,
                        uyarilar,
                        json: Some(deger),
                    }
                }
                Err(e) => {
                    uyari_ekle(&mut uyarilar, &format!("gecersiz JSON: {e}"));
                    AyristirilmisKayit {
                        alanlar: Vec::new(),
                        uyarilar,
                        json: None,
                    }
                }
            },
            Err(_) => {
                uyari_ekle(&mut uyarilar, "gecersiz UTF-8; kayit bosaltildi");
                AyristirilmisKayit {
                    alanlar: Vec::new(),
                    uyarilar,
                    json: None,
                }
            }
        },
    }
}

/// Bir JSON değerini tek sütunlu metin gösterimine çevirir.
///
/// Dizge değerlerinde tırnak eklenmez; diğer türler `serde_json` varsayılan
/// JSON gösterimine çevrilir.
pub fn deger_metni(deger: &serde_json::Value) -> String {
    match deger {
        serde_json::Value::String(s) => s.clone(),
        diger => diger.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn csv(kayit: &[u8]) -> AyristirilmisKayit {
        ayristir(kayit, Bicim::Csv, b',')
    }

    fn jsonl(kayit: &[u8]) -> AyristirilmisKayit {
        ayristir(kayit, Bicim::Jsonl, 0)
    }

    #[test]
    fn duz_satirlar_ayristirilir() {
        let k = csv(b"a,bb,ccc");
        assert_eq!(k.alanlar, vec!["a", "bb", "ccc"]);
        assert!(k.uyarilar.is_empty());
    }

    #[test]
    fn tirnali_alan_ayristirilir() {
        let k = csv(b"\"ali veli\",2");
        assert_eq!(k.alanlar, vec!["ali veli", "2"]);
    }

    #[test]
    fn kacisli_tirnak_ayristirilir() {
        let k = csv(b"\"de\"\"diye\",x");
        assert_eq!(k.alanlar, vec!["de\"diye", "x"]);
    }

    #[test]
    fn tirnak_ici_ayrac_korunur() {
        let k = csv(b"\"a,b\",c");
        assert_eq!(k.alanlar, vec!["a,b", "c"]);
    }

    #[test]
    fn tirnak_ici_crlf_korunur() {
        let k = csv(b"\" satir1\r\nsatir2 \",z");
        assert_eq!(k.alanlar, vec![" satir1\r\nsatir2 ", "z"]);
    }

    #[test]
    fn bos_alanlar_korunur() {
        let k = csv(b",,x,,");
        assert_eq!(k.alanlar, vec!["", "", "x", "", ""]);
    }

    #[test]
    fn tirnaksiz_alanda_tirnak_uyarisi_verir() {
        let k = csv(b"a\"b,c");
        assert_eq!(k.alanlar, vec!["a\"b", "c"]);
        assert!(k.uyarilar.iter().any(|u| u.contains("tirnaksiz")));
    }

    #[test]
    fn kapatilmamis_tirnak_isaretlenir() {
        let k = csv(b"\"acik,x");
        assert!(k.uyarilar.iter().any(|u| u.contains("kapatilmamis")));
        assert_eq!(k.alanlar, vec!["acik,x"]);
    }

    #[test]
    fn tirnak_sonrasi_veri_birlesir() {
        let k = csv(b"\"a\"b,c");
        assert_eq!(k.alanlar, vec!["ab", "c"]);
        assert!(k.uyarilar.iter().any(|u| u.contains("tirnak kapandiktan")));
    }

    #[test]
    fn gecersiz_utf8_isaretlenir() {
        let k = csv(&[b'a', 0xFF, 0xFE, b',', b'b']);
        assert_eq!(k.alanlar, vec!["", "b"]);
        assert!(k.uyarilar.iter().any(|u| u.contains("UTF-8")));
    }

    #[test]
    fn unicode_ve_emoji_ayristirilir() {
        let k = csv("ad,not".as_bytes());
        assert_eq!(k.alanlar, vec!["ad", "not"]);
        let k = csv("İstanbul,🚀 roket".as_bytes());
        assert_eq!(k.alanlar, vec!["İstanbul", "🚀 roket"]);
    }

    #[test]
    fn jsonl_nesne_alanlara_ayrilir() {
        let k = jsonl(br#"{"a":1,"b":"x","c":true}"#);
        assert_eq!(k.alanlar, vec!["1", "x", "true"]);
        assert!(k.uyarilar.is_empty());
    }

    #[test]
    fn jsonl_bozuk_satir_isaretlenir() {
        let k = jsonl(b"{\"a\": }");
        assert!(k.alanlar.is_empty());
        assert!(k.uyarilar.iter().any(|u| u.contains("gecersiz JSON")));
    }

    #[test]
    fn jsonl_dizi_koku_tek_deger_verir() {
        let k = jsonl(b"[1,2,3]");
        assert_eq!(k.alanlar, vec!["[1,2,3]"]);
    }

    #[test]
    fn jsonl_icice_nesne_metne_duser() {
        let k = jsonl(br#"{"a":{"b":1}}"#);
        assert_eq!(k.alanlar, vec![r#"{"b":1}"#.to_string()]);
    }

    #[test]
    fn uyari_metni_birlestirir() {
        let mut k = AyristirilmisKayit::default();
        assert!(k.uyari_metni().is_none());
        k.uyarilar.push("x".to_string());
        k.uyarilar.push("y".to_string());
        assert_eq!(k.uyari_metni().unwrap_or_default(), "x; y");
    }

    // --- kayıt sınırlama ---

    fn cozum(bicim: Bicim, ayrac: u8, dizi_kok: bool) -> KayitCozum {
        KayitCozum {
            bicim,
            ayrac,
            dizi_kok,
        }
    }

    #[test]
    fn csv_siniri_lf_sonrasinda_biter() {
        let t = kayit_tara(b"a,b\nc,d", 0, &cozum(Bicim::Csv, b',', false), true);
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 0,
                son: 3,
                sonraki: 4
            }
        );
    }

    #[test]
    fn csv_siniri_crlf_sonrasinda_biter() {
        let t = kayit_tara(b"a,b\r\nc,d", 0, &cozum(Bicim::Csv, b',', false), true);
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 0,
                son: 3,
                sonraki: 5
            }
        );
    }

    #[test]
    fn csv_siniri_yalnizca_cr_sonrasinda_biter() {
        let t = kayit_tara(b"a,b\rc,d", 0, &cozum(Bicim::Csv, b',', false), true);
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 0,
                son: 3,
                sonraki: 4
            }
        );
    }

    #[test]
    fn tirnak_ici_satir_sonu_kayit_siniri_degistirmez() {
        let t = kayit_tara(b"\"a\nb\",c\nd", 0, &cozum(Bicim::Csv, b',', false), true);
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 0,
                son: 7,
                sonraki: 8
            }
        );
    }

    #[test]
    fn kacisli_tirnak_kayit_siniri_erken_bitirmez() {
        let t = kayit_tara(b"\"a\"\"b\",c\nd", 0, &cozum(Bicim::Csv, b',', false), true);
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 0,
                son: 8,
                sonraki: 9
            }
        );
    }

    #[test]
    fn tirnaksiz_alanda_tirnak_kayit_sinirini_bozmaz() {
        // Alan ortasindaki tirnak veridir; satir sonu kayit bitirir.
        let t = kayit_tara(
            b"a,b\nde\"diye,x\n3,4",
            0,
            &cozum(Bicim::Csv, b',', false),
            true,
        );
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 0,
                son: 3,
                sonraki: 4
            }
        );
        let t2 = kayit_tara(
            b"a,b\nde\"diye,x\n3,4",
            4,
            &cozum(Bicim::Csv, b',', false),
            true,
        );
        assert_eq!(
            t2,
            Tarama::Tamam {
                baslangic: 4,
                son: 13,
                sonraki: 14
            }
        );
    }

    #[test]
    fn ayrac_sonrasi_tirnak_yine_acilistir() {
        let t = kayit_tara(b"a,\"b,c\"\n2,3", 0, &cozum(Bicim::Csv, b',', false), true);
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 0,
                son: 7,
                sonraki: 8
            }
        );
    }

    #[test]
    fn tirnak_sonrasi_veri_kayit_sinirini_bozmaz() {
        let t = kayit_tara(b"\"a\"b,c\n2,3", 0, &cozum(Bicim::Csv, b',', false), true);
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 0,
                son: 6,
                sonraki: 7
            }
        );
    }

    #[test]
    fn son_isareti_yoksa_son_bayta_kadar_alinir() {
        let t = kayit_tara(b"a,b", 0, &cozum(Bicim::Csv, b',', false), true);
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 0,
                son: 3,
                sonraki: 3
            }
        );
    }

    #[test]
    fn tampon_bittiginde_devam_doner() {
        let t = kayit_tara(b"\"a", 0, &cozum(Bicim::Csv, b',', false), false);
        assert_eq!(t, Tarama::Devam);
    }

    #[test]
    fn tampon_sonunda_cr_beklenir() {
        let t = kayit_tara(b"a\r", 0, &cozum(Bicim::Csv, b',', false), false);
        assert_eq!(t, Tarama::Devam);
    }

    #[test]
    fn tampon_sonunda_kacisli_tirnak_beklenir() {
        let t = kayit_tara(b"\"a\"", 0, &cozum(Bicim::Csv, b',', false), false);
        assert_eq!(t, Tarama::Devam);
    }

    #[test]
    fn jsonl_dizge_ici_satir_sonu_yoksayilir() {
        let t = kayit_tara(
            b"{\"a\":\"x\\ny\"}\n{}",
            0,
            &cozum(Bicim::Jsonl, 0, false),
            true,
        );
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 0,
                son: 12,
                sonraki: 13
            }
        );
    }

    #[test]
    fn jsonl_satiri_crlf_ile_biter() {
        let t = kayit_tara(b"{}\r\n{}", 0, &cozum(Bicim::Jsonl, 0, false), true);
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 0,
                son: 2,
                sonraki: 4
            }
        );
    }

    #[test]
    fn jsonl_dizi_elemanlari_ayrilir() {
        let c = cozum(Bicim::Jsonl, 0, true);
        let t = kayit_tara(br#"[{"a":1},{"b":2}]"#, 1, &c, true);
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 1,
                son: 8,
                sonraki: 9
            }
        );
        let t2 = kayit_tara(br#"[{"a":1},{"b":2}]"#, 9, &c, true);
        assert_eq!(
            t2,
            Tarama::Tamam {
                baslangic: 9,
                son: 16,
                sonraki: 16
            }
        );
        let t3 = kayit_tara(br#"[{"a":1},{"b":2}]"#, 16, &c, true);
        assert_eq!(t3, Tarama::DiziBitti);
    }

    #[test]
    fn jsonl_dizi_icine_gomulu_dizi_ayrilir() {
        let c = cozum(Bicim::Jsonl, 0, true);
        let t = kayit_tara(b"[[1,2],[3,4],5]", 1, &c, true);
        assert_eq!(
            t,
            Tarama::Tamam {
                baslangic: 1,
                son: 6,
                sonraki: 7
            }
        );
        let t2 = kayit_tara(b"[[1,2],[3,4],5]", 7, &c, true);
        assert_eq!(
            t2,
            Tarama::Tamam {
                baslangic: 7,
                son: 12,
                sonraki: 13
            }
        );
    }
}
