//! Biçim (CSV / TSV / JSONL) ve alan ayracı algılama.
//!
//! Sorumluluğu "hangi biçim ve hangi ayraç?" sorusunu, dosyayı okumadan önce
//! cevaplamaktır. Bu modül saf fonksiyonlardan oluşur: dosya sistemine dokunmaz,
//! yalnızca verilen bayt dizisine bakar. I/O'yu `kaynak` modülü yapar.
//!
//! Ne **değil**: bu modül içeriği ayrıştırmaz; ayrıştırma `ayristirici`
//! modülündedir.

use std::path::Path;

use serde::Serialize;

/// CSV ayracı olarak kabul edilen baytların aday listesi.
///
/// Rapor (b07) dört ayraç belirtir: virgül, noktalı virgül, sekme ve boru.
/// Liste sırası eşitlik durumunda tercih sırasını belirler.
pub const ADAYRACLAR: [u8; 4] = *b",;\t|";

/// `kaynak` modülünün kullandığı dosya biçimleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Bicim {
    /// Virgül / ayraçlı düz metin; RFC 4180 kuralları uygulanır.
    Csv,
    /// Sekme ayraçlı düz metin. CSV ile aynı ayrıştırıcıyı kullanır.
    Tsv,
    /// Satır başına tek JSON değeri (JSON Lines) veya kökü JSON dizi olan dosya.
    Jsonl,
}

impl Bicim {
    /// Biçimin adı; JSON çıktısında ve hata mesajlarında kullanılır.
    pub fn ad(self) -> &'static str {
        match self {
            Bicim::Csv => "csv",
            Bicim::Tsv => "tsv",
            Bicim::Jsonl => "jsonl",
        }
    }

    /// Biçimin kendi varsayılan ayracı. JSONL'de ayraç yoktur.
    pub fn ayrac_varsayilan(self) -> u8 {
        match self {
            Bicim::Csv => b',',
            Bicim::Tsv => b'\t',
            Bicim::Jsonl => 0,
        }
    }

    /// Başlık satırı olup olmadığının varsayılanı.
    ///
    /// CSV/TSV'de ilk satır başlıktır. JSONL'de başlık satırı yoktur; sütun
    /// adları kayıtların anahtarlarından türetilir.
    pub fn baslik_varsayilan(self) -> bool {
        match self {
            Bicim::Csv | Bicim::Tsv => true,
            Bicim::Jsonl => false,
        }
    }

    /// Ayracın bu biçimde anlamlı olup olmadığını söyler.
    pub fn ayrac_gecerli(self, ayrac: u8) -> bool {
        match self {
            Bicim::Jsonl => ayrac == 0,
            Bicim::Csv | Bicim::Tsv => ADAYRACLAR.contains(&ayrac),
        }
    }
}

/// Dosya adı uzantısından biçim çıkarır.
///
/// Tanınmayan uzantı için `None` döner; çağıran taraf içerikten algılamayı dener.
pub fn uzanti_bicimi(yol: &Path) -> Option<Bicim> {
    let uzanti = yol.extension()?.to_str()?.to_ascii_lowercase();
    match uzanti.as_str() {
        "csv" | "txt" => Some(Bicim::Csv),
        "tsv" | "tab" => Some(Bicim::Tsv),
        "jsonl" | "ndjson" | "jsonlines" => Some(Bicim::Jsonl),
        _ => None,
    }
}

/// Dosyanın ilk baytlarından biçim çıkarır.
///
/// İlk anlamlı bayt `{` veya `[` ise JSONL kabul edilir; aksi hâlde içerik
/// CSV kabul edilir. Bu sezgisel bir varsayımdır ve `--bicim` ile geçersiz
/// kılınabilir.
pub fn icerik_bicimi(ilk_baytlar: &[u8]) -> Option<Bicim> {
    match ilk_baytlar.iter().find(|b| !b.is_ascii_whitespace())? {
        b'{' | b'[' => Some(Bicim::Jsonl),
        _ => Some(Bicim::Csv),
    }
}

/// Bir kaydın içeriğinde tırnak dışında geçen aday ayraçları sayar.
///
/// Tırnak içindeki ayraçlar sayılmaz; böylece `"a,b"` yazan bir sütun başlığı
/// yanlış ayraç sayımına yol açmaz.
fn aday_sayimlari(kayit: &[u8]) -> [u32; ADAYRACLAR.len()] {
    let mut sayimlar = [0u32; ADAYRACLAR.len()];
    let mut tirnakli = false;
    let mut i = 0usize;
    while i < kayit.len() {
        let b = kayit[i];
        if b == b'"' {
            if tirnakli && i + 1 < kayit.len() && kayit[i + 1] == b'"' {
                i += 2;
                continue;
            }
            tirnakli = !tirnakli;
            i += 1;
            continue;
        }
        if !tirnakli {
            if let Some(sira) = ADAYRACLAR.iter().position(|a| *a == b) {
                sayimlar[sira] = sayimlar[sira].saturating_add(1);
            }
        }
        i += 1;
    }
    sayimlar
}

/// İlk kayıtın içeriğinden en olası ayracı seçer.
///
/// Tüm adaylar sıfırsa (tek sütunlu dosya) `varsayilan` döner: bu durumda
/// hangi ayracın seçildiği ayrıştırma sonucunu değiştirmez.
pub fn ayrac_se(kayit: &[u8], varsayilan: u8) -> u8 {
    let sayimlar = aday_sayimlari(kayit);
    let mut en_iyi = 0usize;
    let mut en_cok = 0u32;
    for (sira, adet) in sayimlar.iter().enumerate() {
        if *adet > en_cok {
            en_cok = *adet;
            en_iyi = sira;
        }
    }
    if en_cok == 0 {
        varsayilan
    } else {
        ADAYRACLAR[en_iyi]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn uzantilar_bicime_eslenir() {
        assert_eq!(uzanti_bicimi(&PathBuf::from("a.csv")), Some(Bicim::Csv));
        assert_eq!(uzanti_bicimi(&PathBuf::from("a.CSV")), Some(Bicim::Csv));
        assert_eq!(uzanti_bicimi(&PathBuf::from("a.tsv")), Some(Bicim::Tsv));
        assert_eq!(uzanti_bicimi(&PathBuf::from("a.tab")), Some(Bicim::Tsv));
        assert_eq!(
            uzanti_bicimi(&PathBuf::from("a.ndjson")),
            Some(Bicim::Jsonl)
        );
        assert_eq!(uzanti_bicimi(&PathBuf::from("a.bin")), None);
        assert_eq!(uzanti_bicimi(&PathBuf::from("uzantisiz")), None);
    }

    #[test]
    fn icerikten_jsonl_algilanir() {
        assert_eq!(icerik_bicimi(b"  \n{\"a\":1}"), Some(Bicim::Jsonl));
        assert_eq!(icerik_bicimi(b"[{\"a\":1}]"), Some(Bicim::Jsonl));
    }

    #[test]
    fn icerikten_csv_algilanir() {
        assert_eq!(icerik_bicimi(b"a,b\n1,2"), Some(Bicim::Csv));
    }

    #[test]
    fn bos_icerik_bicim_algilamaz() {
        assert_eq!(icerik_bicimi(b"   \n\t\n"), None);
    }

    #[test]
    fn virgul_ayraci_secilir() {
        assert_eq!(ayrac_se(b"a,b,c", b','), b',');
    }

    #[test]
    fn noktali_virgul_ayraci_secilir() {
        assert_eq!(ayrac_se(b"a;b;c;d", b','), b';');
    }

    #[test]
    fn sekme_ayraci_secilir() {
        assert_eq!(ayrac_se(b"a\tb\tc", b','), b'\t');
    }

    #[test]
    fn boru_ayraci_secilir() {
        assert_eq!(ayrac_se(b"a|b|c", b','), b'|');
    }

    #[test]
    fn tirnak_ici_ayrac_sayilmaz() {
        // Tirnak disinda yalnizca bir ayrac var; tirnak ici olan sayilmiyor.
        assert_eq!(ayrac_se(b"\"a,b;c\";d", b','), b';');
    }

    #[test]
    fn kacisli_tirnak_sayimi_bozmaz() {
        assert_eq!(ayrac_se(b"\"a\"\"b\";x;y", b','), b';');
    }

    #[test]
    fn aday_yoksa_varsayilan_doner() {
        assert_eq!(ayrac_se(b"tek_sutun", b'\t'), b'\t');
    }

    #[test]
    fn esitlikte_ilk_aday_kazanir() {
        // Virgul ve noktali virgul esit sayida: liste sirasi belirler.
        assert_eq!(ayrac_se(b"a,b;c", b'|'), b',');
    }

    #[test]
    fn bicim_ozellikleri_bicimin_kendisine_bagli() {
        assert_eq!(Bicim::Csv.ad(), "csv");
        assert_eq!(Bicim::Tsv.ayrac_varsayilan(), b'\t');
        assert!(Bicim::Csv.baslik_varsayilan());
        assert!(!Bicim::Jsonl.baslik_varsayilan());
        assert!(Bicim::Csv.ayrac_gecerli(b'|'));
        assert!(!Bicim::Csv.ayrac_gecerli(b'\n'));
        assert!(Bicim::Jsonl.ayrac_gecerli(0));
    }
}
