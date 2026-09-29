//! Satır erişim servisi: biçim çözümleme, sütun çözümleme ve konum bazlı okuma.
//!
//! Bu modül raporun b06 şemasındaki "Satır erişim servisi" katmanıdır. Görevleri:
//!
//! 1. Dosyayı açmak ve biçim/ayraç kararlarını vermek,
//! 2. Satır indeksini kurmak (birinci geçiş),
//! 3. Sütun adlarını çözümlemek,
//! 4. İstenen satırları `seek` ile okumak (ikinci geçiş) ve alanlara ayırmak.
//!
//! Ne **değil**: profilleme (`profil`), filtreleme (`filtre`) ve gösterim
//! (`gorunum`) bu modülün sorumluluğunda değildir.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::ayristirici::{ayristir, deger_metni, AyristirilmisKayit, KayitCozum};
use crate::bicim::{ayrac_se, icerik_bicimi, uzanti_bicimi, Bicim};
use crate::hata::{io_hata, Hata};
use crate::indeks::{aralik_bul, kur, IndeksKipi, Kaydirma, KayitAraligi, Sablon, SatirIndeksi};

/// Sütun adlarını çözümlemek için JSONL örneklemesinde bakılacak kayıt sayısı.
const JSONL_ANAHTAR_ORNEGI: u64 = 200;

/// Biçim ve ayraç sezgisinde kullanılan ön izleme boyutu.
const ONIZLEME_BOYUTU: usize = 64 * 1024;

/// Dosyayı açma ve okuma seçenekleri.
#[derive(Debug, Clone)]
pub struct Ayarlar {
    /// Biçimi elle seç; `None` ise uzantıdan, sonra içerikten algılanır.
    pub bicim: Option<Bicim>,
    /// Ayraç baytını elle seç; `None` ise ilk kayıttan algılanır.
    pub ayrac: Option<u8>,
    /// Başlık satırı olup olmadığı; `None` ise biçimin varsayılanı.
    pub baslik: Option<bool>,
    /// Satır indeksinin kipi.
    pub indeks: IndeksKipi,
    /// Tampon boyutları.
    pub sablon: Sablon,
}

impl Default for Ayarlar {
    fn default() -> Self {
        Ayarlar {
            bicim: None,
            ayrac: None,
            baslik: None,
            indeks: IndeksKipi::Seyrek,
            sablon: Sablon::default(),
        }
    }
}

/// Dosyanın statik kimlik bilgileri; profil çıktısında raporlanır.
#[derive(Debug, Clone, Serialize)]
pub struct DosyaBilgisi {
    /// Verilen yol.
    pub yol: String,
    /// Dosya adı.
    pub ad: String,
    /// Dosya boyutu (bayt).
    pub boyut_bayt: u64,
    /// Çözümlenen biçim adı.
    pub bicim: &'static str,
    /// CSV/TSV ayracının okunabilir gösterimi.
    pub ayrac: Option<String>,
    /// Başlık satırı var mı?
    pub baslik_var: bool,
    /// Kullanılan indeks kipi.
    pub indeks_kipi: &'static str,
    /// Seyrek kipte blok satır sayısı.
    pub blok_satir: usize,
    /// JSONL dosyasının kökü JSON dizi mi?
    pub dizi_kok: bool,
    /// İndeksin tahminî bellek tüketimi (bayt).
    pub indeks_bellek_bayt: u64,
}

/// İki geçişte okunan bayt miktarı.
///
/// Bu sayaçlar "sabit bellek" iddiasının **ölçülebilir** kanıtıdır: `veri_bayt`
/// dosya boyutundan değil, istenen satır penceresinden bağımsızdır.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct OkumaIstatistik {
    /// Birinci geçişte (indizleme) okunan bayt. Dosyanın tamamıdır ama sabit
    /// tamponla akış hâlinde okunur, belleğe alınmaz.
    pub indeks_bayt: u64,
    /// İkinci geçişte (satır okuma) okunan bayt. Dosya boyutundan bağımsızdır.
    pub veri_bayt: u64,
    /// Yapılan `seek` çağrısı sayısı.
    pub seek_sayisi: u64,
}

/// Okunmuş tek bir satır.
#[derive(Debug, Clone, Serialize)]
pub struct Satir {
    /// Sıfırdan başlayan satır numarası.
    pub no: u64,
    /// Başlıklara hizalı alan değerleri. CSV kaynağında alan sayısı başlık
    /// sayısından farklıysa uzunluk farklı olabilir; uyarı alanına yazılır.
    pub alanlar: Vec<String>,
    /// JSONL kaynağında ayrıştırılmış özgün JSON değeri; CSV kaynağında `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json: Option<serde_json::Value>,
    /// Satırla ilgili uyarı (bozuk JSON, tırnak hatası, sütun sayısı uyuşmazlığı).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uyari: Option<String>,
}

/// Sabit bellekle çalışan satır erişim servisi.
///
/// Açılışta dosya iki kez dokunulur: önce baştan sona akış hâlinde indeks kurulur,
/// sonra yalnızca istenen satırlar konum bazlı okunur.
pub struct Kaynak {
    okuyucu: Kaydirma<File>,
    indeks: SatirIndeksi,
    basliklar: Vec<String>,
    bilgi: DosyaBilgisi,
    indeks_bayt: u64,
}

impl Kaynak {
    /// Dosyayı açar, biçim ve sütun çözümlemesini yapar, satır indeksini kurar.
    pub fn ac(yol: &Path, ayar: Ayarlar) -> Result<Kaynak, Hata> {
        let meta = std::fs::metadata(yol).map_err(|e| io_hata("istatistik alma", yol, e))?;
        if !meta.is_file() {
            return Err(Hata::DosyaDegil {
                yol: yol.to_path_buf(),
            });
        }
        let boyut = meta.len();

        let onizleme = onizleme_al(yol)?;
        let bicim = bicim_coz(yol, &onizleme, ayar.bicim)?;
        let ayrac = ayrac_coz(bicim, &onizleme, ayar.ayrac)?;

        let indeks_dosya = File::open(yol).map_err(|e| io_hata("acma", yol, e))?;
        let indeks = kur(
            indeks_dosya,
            yol,
            bicim,
            ayrac,
            ayar.indeks,
            ayar.sablon,
            boyut,
        )?;

        let baslik_var = ayar.baslik.unwrap_or_else(|| bicim.baslik_varsayilan());
        let veri_dosya = File::open(yol).map_err(|e| io_hata("acma", yol, e))?;
        let mut okuyucu = Kaydirma::ac(veri_dosya, ayar.sablon.veri_tampon, yol);
        let basliklar = basliklari_coz(&mut okuyucu, &indeks, baslik_var)?;

        let bilgi = DosyaBilgisi {
            yol: yol.to_string_lossy().into_owned(),
            ad: yol
                .file_name()
                .map(|a| a.to_string_lossy().into_owned())
                .unwrap_or_default(),
            boyut_bayt: boyut,
            bicim: bicim.ad(),
            ayrac: ayrac_goster(bicim, ayrac),
            baslik_var,
            indeks_kipi: indeks.kip().ad(),
            blok_satir: indeks.blok_satir(),
            dizi_kok: indeks.dizi_kok(),
            indeks_bellek_bayt: indeks.bellek_tahmini(),
        };

        Ok(Kaynak {
            okuyucu,
            indeks,
            basliklar,
            bilgi,
            indeks_bayt: boyut,
        })
    }

    /// Dosyanın statik bilgileri.
    pub fn bilgi(&self) -> &DosyaBilgisi {
        &self.bilgi
    }

    /// Toplam veri satırı sayısı (başlık satırı dahil değildir).
    pub fn satir_sayisi(&self) -> u64 {
        if self.baslik_var_mi() {
            self.indeks.satir_sayisi().saturating_sub(1)
        } else {
            self.indeks.satir_sayisi()
        }
    }

    /// Başlık satırının indekste yer alıp almadığı.
    pub fn baslik_var_mi(&self) -> bool {
        self.bilgi.baslik_var && self.indeks.satir_sayisi() > 0
    }

    /// İndeksleme sırasında atlanan tamamen boş satır sayısı.
    pub fn atlanan_bos_satir(&self) -> u64 {
        self.indeks.atlanan_bos_satir()
    }

    /// Çözümlenen sütun adları.
    pub fn basliklar(&self) -> &[String] {
        &self.basliklar
    }

    /// Sütun adının sıra numarasını döndürür (bulunamazsa `None`).
    ///
    /// Ad tam eşleşme, büyük/küçük harf duyarsız eşleşme ve son olarak sıra numarası
    /// olarak çözülür.
    pub fn sutun_sira(&self, ad: &str) -> Option<usize> {
        if let Some(i) = self.basliklar.iter().position(|b| b == ad) {
            return Some(i);
        }
        let kucuk = ad.to_lowercase();
        self.basliklar
            .iter()
            .position(|b| b.to_lowercase() == kucuk)
            .or_else(|| {
                ad.parse::<usize>()
                    .ok()
                    .filter(|i| *i < self.basliklar.len())
            })
    }

    /// Okuma sayaclarını döndürür.
    pub fn istatistik(&self) -> OkumaIstatistik {
        let (veri_bayt, seek_sayisi) = self.okuyucu.sayaclar();
        OkumaIstatistik {
            indeks_bayt: self.indeks_bayt,
            veri_bayt,
            seek_sayisi,
        }
    }

    /// `no` numaralı veri satırını okur.
    pub fn satir(&mut self, no: u64) -> Result<Satir, Hata> {
        if no >= self.satir_sayisi() {
            return Err(Hata::GecersizAralik {
                baslangic: no,
                adet: 1,
                satir_sayisi: self.satir_sayisi(),
            });
        }
        let indeks_no = if self.baslik_var_mi() { no + 1 } else { no };
        let cozum = self.indeks.cozum();
        let aralik = aralik_bul(&mut self.okuyucu, &self.indeks, indeks_no)?;
        let baytlar = kayit_baytlar(&mut self.okuyucu, aralik)?;
        Ok(self.satir_olustur(no, &baytlar, cozum))
    }

    /// `[baslangic, baslangic + adet)` aralığındaki satırları okur.
    pub fn pencere(&mut self, baslangic: u64, adet: u64) -> Result<Vec<Satir>, Hata> {
        if baslangic > self.satir_sayisi() {
            return Err(Hata::GecersizAralik {
                baslangic,
                adet,
                satir_sayisi: self.satir_sayisi(),
            });
        }
        let son = baslangic.saturating_add(adet).min(self.satir_sayisi());
        let mut sonuc = Vec::with_capacity((son - baslangic) as usize);
        for no in baslangic..son {
            sonuc.push(self.satir(no)?);
        }
        Ok(sonuc)
    }

    /// Ham kaydı ayrıştırıp başlıklara hizalar.
    fn satir_olustur(&self, no: u64, baytlar: &[u8], cozum: KayitCozum) -> Satir {
        let AyristirilmisKayit {
            alanlar,
            mut uyarilar,
            json,
        } = ayristir(baytlar, cozum.bicim, cozum.ayrac);

        let mut hizali: Vec<String> = Vec::with_capacity(self.basliklar.len());
        match &json {
            Some(serde_json::Value::Object(harita)) => {
                for ad in &self.basliklar {
                    hizali.push(harita.get(ad).map(deger_metni).unwrap_or_default());
                }
            }
            Some(tek) => hizali.push(deger_metni(tek)),
            None => {
                if cozum.bicim == Bicim::Jsonl {
                    hizali = vec![String::new(); self.basliklar.len()];
                } else {
                    if alanlar.len() != self.basliklar.len() {
                        uyarilar.push(format!(
                            "sutun sayisi uyusmuyor (baslik {}, satir {})",
                            self.basliklar.len(),
                            alanlar.len()
                        ));
                    }
                    hizali = alanlar;
                }
            }
        }
        uyarilar.sort();
        uyarilar.dedup();
        Satir {
            no,
            alanlar: hizali,
            json,
            uyari: if uyarilar.is_empty() {
                None
            } else {
                Some(uyarilar.join("; "))
            },
        }
    }
}

/// `Kaynak` elle `Debug` uygular: `Kaydirma<File>` alanı türetilemez ve hata
/// ayıklama çıktısında dosya tanıtıcısının kendisi anlamlı değildir.
impl std::fmt::Debug for Kaynak {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kaynak")
            .field("bilgi", &self.bilgi)
            .field("basliklar", &self.basliklar)
            .finish_non_exhaustive()
    }
}

/// Bir kaydın baytlarını konum bazlı okur; dosyanın tamamını belleğe almaz.
///
/// Uzun kayıtlarda okuyucunun tamponu büyür; büyüme tek bir kayıtla sınırlıdır.
fn kayit_baytlar(okuyucu: &mut Kaydirma<File>, aralik: KayitAraligi) -> Result<Vec<u8>, Hata> {
    okuyucu
        .konumlan(aralik.baslangic)
        .map_err(|e| okuyucu.hata("okuma", e))?;
    let hedef = aralik.uzunluk as usize;
    while okuyucu.uzunluk() < hedef && !okuyucu.eof() {
        okuyucu.uzat().map_err(|e| okuyucu.hata("okuma", e))?;
    }
    let mevcut = okuyucu.uzunluk().min(hedef);
    Ok(okuyucu.baytlar()[..mevcut].to_vec())
}

/// Ayraç baytını okunabilir biçime çevirir; JSONL'de ayraç yoktur.
fn ayrac_goster(bicim: Bicim, ayrac: u8) -> Option<String> {
    if bicim == Bicim::Jsonl {
        return None;
    }
    Some(match ayrac {
        b',' => ",".to_string(),
        b';' => ";".to_string(),
        b'\t' => "\\t".to_string(),
        b'|' => "|".to_string(),
        diger => format!("0x{diger:02x}"),
    })
}

/// Dosyanın başındaki baytları okur (biçim ve ayraç sezgisi için).
fn onizleme_al(yol: &Path) -> Result<Vec<u8>, Hata> {
    let mut dosya = File::open(yol).map_err(|e| io_hata("acma", yol, e))?;
    let mut tampon = vec![0u8; ONIZLEME_BOYUTU];
    let mut toplam = 0usize;
    while toplam < tampon.len() {
        match dosya.read(&mut tampon[toplam..]) {
            Ok(0) => break,
            Ok(n) => toplam += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(io_hata("okuma", yol, e)),
        }
    }
    tampon.truncate(toplam);
    Ok(tampon)
}

/// Biçimi uzantıdan, yoksa içerikten çözer.
fn bicim_coz(yol: &Path, onizleme: &[u8], elle: Option<Bicim>) -> Result<Bicim, Hata> {
    if let Some(b) = elle {
        return Ok(b);
    }
    if let Some(b) = uzanti_bicimi(yol) {
        return Ok(b);
    }
    icerik_bicimi(onizleme).ok_or_else(|| Hata::BilinmeyenBicim {
        yol: yol.to_path_buf(),
    })
}

/// CSV/TSV ayracını ilk kayıttan çözer; JSONL'de ayraç yoktur.
fn ayrac_coz(bicim: Bicim, onizleme: &[u8], elle: Option<u8>) -> Result<u8, Hata> {
    if bicim == Bicim::Jsonl {
        return Ok(0);
    }
    if let Some(a) = elle {
        if !bicim.ayrac_gecerli(a) {
            return Err(Hata::GecersizAyrac {
                deger: format!("0x{a:02x}"),
            });
        }
        return Ok(a);
    }
    Ok(ayrac_se(onizleme, bicim.ayrac_varsayilan()))
}

/// Sütun adlarını çözer.
fn basliklari_coz(
    okuyucu: &mut Kaydirma<File>,
    indeks: &SatirIndeksi,
    baslik_var: bool,
) -> Result<Vec<String>, Hata> {
    let cozum = indeks.cozum();
    if cozum.bicim == Bicim::Jsonl {
        return jsonl_anahtarlari(okuyucu, indeks);
    }
    let toplam = indeks.satir_sayisi();
    if toplam == 0 {
        return Ok(Vec::new());
    }
    let aralik = aralik_bul(okuyucu, indeks, 0)?;
    let baytlar = kayit_baytlar(okuyucu, aralik)?;
    let kayit = ayristir(&baytlar, cozum.bicim, cozum.ayrac);
    if baslik_var {
        Ok(adlari_basarit(kayit.alanlar))
    } else {
        Ok((1..=kayit.alanlar.len())
            .map(|i| format!("sutun_{i}"))
            .collect())
    }
}

/// JSONL kayıtlarının nesne anahtarlarını ilk görülme sırasıyla toplar.
fn jsonl_anahtarlari(
    okuyucu: &mut Kaydirma<File>,
    indeks: &SatirIndeksi,
) -> Result<Vec<String>, Hata> {
    let toplam = indeks.satir_sayisi().min(JSONL_ANAHTAR_ORNEGI);
    let mut anahtarlar: Vec<String> = Vec::new();
    for no in 0..toplam {
        let aralik = aralik_bul(okuyucu, indeks, no)?;
        let baytlar = kayit_baytlar(okuyucu, aralik)?;
        if let Ok(serde_json::Value::Object(harita)) =
            serde_json::from_slice::<serde_json::Value>(&baytlar)
        {
            for ad in harita.keys() {
                if !anahtarlar.contains(ad) {
                    anahtarlar.push(ad.clone());
                }
            }
        }
    }
    if anahtarlar.is_empty() && toplam > 0 {
        return Ok(vec!["deger".to_string()]);
    }
    Ok(anahtarlar)
}

/// Boş ve yinelenen sütun adlarını benzersiz hâle getirir.
fn adlari_basarit(adlar: Vec<String>) -> Vec<String> {
    let mut sonuc: Vec<String> = Vec::with_capacity(adlar.len());
    for (i, ham) in adlar.iter().enumerate() {
        let temel = if ham.trim().is_empty() {
            format!("sutun_{}", i + 1)
        } else {
            ham.trim().to_string()
        };
        let mut adet = 1usize;
        let mut ad = temel.clone();
        while sonuc.contains(&ad) {
            adet += 1;
            ad = format!("{temel}_{adet}");
        }
        sonuc.push(ad);
    }
    sonuc
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::indeks::IndeksKipi;
    use crate::test_yardimcisi::GeciciDizin;
    use std::io::Write;

    fn yaz(klasor: &Path, ad: &str, icerik: &[u8]) -> std::path::PathBuf {
        let yol = klasor.join(ad);
        let mut f = File::create(&yol).expect("olustur");
        f.write_all(icerik).expect("yaz");
        f.flush().expect("kapat");
        yol
    }

    fn ac(yol: &Path) -> Kaynak {
        Kaynak::ac(yol, Ayarlar::default()).expect("ac")
    }

    #[test]
    fn csv_baslik_ve_veriler_okunur() {
        let d = GeciciDizin::yeni("dl-kaynak-csv").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"ad,yas\nali,30\nayse,41\n");
        let mut k = ac(&yol);
        assert_eq!(k.basliklar(), &["ad".to_string(), "yas".to_string()]);
        assert_eq!(k.satir_sayisi(), 2);
        assert_eq!(k.satir(0).unwrap().alanlar, vec!["ali", "30"]);
        assert_eq!(k.satir(1).unwrap().no, 1);
    }

    #[test]
    fn basliksiz_csv_sutunlari_uretir() {
        let d = GeciciDizin::yeni("dl-kaynak-basliksiz").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"1,2,3\n4,5,6\n");
        let ayar = Ayarlar {
            baslik: Some(false),
            ..Ayarlar::default()
        };
        let k = Kaynak::ac(&yol, ayar).expect("ac");
        assert_eq!(
            k.basliklar(),
            &[
                "sutun_1".to_string(),
                "sutun_2".to_string(),
                "sutun_3".to_string()
            ]
        );
        assert_eq!(k.satir_sayisi(), 2);
    }

    #[test]
    fn yinelenen_ve_bos_basliklar_basaritilir() {
        let d = GeciciDizin::yeni("dl-kaynak-basarit").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a,,a,a\n1,2,3,4\n");
        let k = ac(&yol);
        assert_eq!(
            k.basliklar(),
            &[
                "a".to_string(),
                "sutun_2".to_string(),
                "a_2".to_string(),
                "a_3".to_string()
            ]
        );
    }

    #[test]
    fn tsv_dosyasi_sekme_ayraciyla_okunur() {
        let d = GeciciDizin::yeni("dl-kaynak-tsv").expect("dizin");
        let yol = yaz(d.yol(), "a.tsv", b"a\tb\n1\t2\n");
        let k = ac(&yol);
        assert_eq!(k.bilgi().bicim, "tsv");
        assert_eq!(k.basliklar(), &["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn noktali_virgul_ayraci_otomatik_bulunur() {
        let d = GeciciDizin::yeni("dl-kaynak-noktali").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a;b;c\n1;2;3\n");
        let k = ac(&yol);
        assert_eq!(k.basliklar().len(), 3);
        assert_eq!(k.bilgi().ayrac.as_deref(), Some(";"));
    }

    #[test]
    fn jsonl_anahtarlarindan_sutun_uretir() {
        let d = GeciciDizin::yeni("dl-kaynak-jsonl").expect("dizin");
        let yol = yaz(
            d.yol(),
            "a.jsonl",
            b"{\"a\":1,\"b\":\"x\"}\n{\"a\":2,\"c\":true}\n",
        );
        let mut k = ac(&yol);
        assert_eq!(
            k.basliklar(),
            &["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert_eq!(k.satir(1).unwrap().alanlar, vec!["2", "", "true"]);
    }

    #[test]
    fn jsonl_dizi_koku_okunur() {
        let d = GeciciDizin::yeni("dl-kaynak-dizi").expect("dizin");
        let yol = yaz(d.yol(), "a.json", br#"[{"a":1},{"a":2}]"#);
        let mut k = ac(&yol);
        assert!(k.bilgi().dizi_kok);
        assert_eq!(k.satir_sayisi(), 2);
        assert_eq!(k.satir(0).unwrap().alanlar, vec!["1"]);
    }

    #[test]
    fn jsonl_bozuk_satir_uyariyla_gelir() {
        let d = GeciciDizin::yeni("dl-kaynak-bozuk").expect("dizin");
        let yol = yaz(d.yol(), "a.jsonl", b"{\"a\":1}\nbozuk\n{\"a\":2}\n");
        let mut k = ac(&yol);
        assert_eq!(k.satir_sayisi(), 3);
        let bozuk = k.satir(1).unwrap();
        assert!(bozuk.uyari.unwrap().contains("gecersiz JSON"));
        assert_eq!(k.satir(2).unwrap().alanlar, vec!["2"]);
    }

    #[test]
    fn eksik_sutun_uyari_uretir() {
        let d = GeciciDizin::yeni("dl-kaynak-eksik").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a,b,c\n1,2\n");
        let mut k = ac(&yol);
        let satir = k.satir(0).unwrap();
        assert_eq!(satir.alanlar.len(), 2);
        assert!(satir.uyari.unwrap().contains("sutun sayisi uyusmuyor"));
    }

    #[test]
    fn fazla_sutun_aynen_korunur() {
        let d = GeciciDizin::yeni("dl-kaynak-fazla").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a,b\n1,2,3,4\n");
        let mut k = ac(&yol);
        let satir = k.satir(0).unwrap();
        assert_eq!(satir.alanlar, vec!["1", "2", "3", "4"]);
        assert!(satir.uyari.unwrap().contains("sutun sayisi uyusmuyor"));
    }

    #[test]
    fn iki_kip_ayni_veriyi_verir() {
        let d = GeciciDizin::yeni("dl-kaynak-kip").expect("dizin");
        let mut govde = String::from("no,ad\n");
        for i in 0..1200 {
            govde.push_str(&format!("{i},ad{i}\n"));
        }
        let yol = yaz(d.yol(), "a.csv", govde.as_bytes());
        for kip in [IndeksKipi::Yogun, IndeksKipi::Seyrek] {
            let ayar = Ayarlar {
                indeks: kip,
                ..Ayarlar::default()
            };
            let mut k = Kaynak::ac(&yol, ayar).expect("ac");
            assert_eq!(k.satir_sayisi(), 1200, "kip {kip:?}");
            assert_eq!(k.satir(0).unwrap().alanlar, vec!["0", "ad0"]);
            assert_eq!(k.satir(1199).unwrap().alanlar, vec!["1199", "ad1199"]);
        }
    }

    #[test]
    fn pencere_araligi_okur() {
        let d = GeciciDizin::yeni("dl-kaynak-pencere").expect("dizin");
        let mut govde = String::from("no\n");
        for i in 0..100 {
            govde.push_str(&format!("{i}\n"));
        }
        let yol = yaz(d.yol(), "a.csv", govde.as_bytes());
        let mut k = ac(&yol);
        let satirlar = k.pencere(10, 5).expect("pencere");
        assert_eq!(satirlar.len(), 5);
        assert_eq!(satirlar[0].alanlar, vec!["10"]);
        assert_eq!(satirlar[4].no, 14);
    }

    #[test]
    fn pencere_sonu_kirpilmasi() {
        let d = GeciciDizin::yeni("dl-kaynak-sonu").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a\n1\n2\n");
        let mut k = ac(&yol);
        assert_eq!(k.pencere(1, 99).expect("pencere").len(), 1);
    }

    #[test]
    fn pencere_son_disinda_hata_verir() {
        let d = GeciciDizin::yeni("dl-kaynak-penceredi").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a\n1\n");
        let mut k = ac(&yol);
        let hata = k.pencere(5, 1).expect_err("hata");
        assert!(matches!(hata, Hata::GecersizAralik { .. }));
    }

    #[test]
    fn satir_diisi_hata_verir() {
        let d = GeciciDizin::yeni("dl-kaynak-diisi").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a\n1\n");
        let mut k = ac(&yol);
        assert!(k.satir(9).is_err());
    }

    #[test]
    fn sutun_sirasi_ad_ve_indeksle_cozulur() {
        let d = GeciciDizin::yeni("dl-kaynak-sirano").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"ad,yas\n1,2\n");
        let k = ac(&yol);
        assert_eq!(k.sutun_sira("yas"), Some(1));
        assert_eq!(k.sutun_sira("YAS"), Some(1));
        assert_eq!(k.sutun_sira("0"), Some(0));
        assert_eq!(k.sutun_sira("olmayan"), None);
        assert_eq!(k.sutun_sira("9"), None);
    }

    #[test]
    fn olmayan_yol_hata_verir() {
        let hata = Kaynak::ac(
            std::path::Path::new("bulunmayan-dosya.csv"),
            Ayarlar::default(),
        )
        .expect_err("hata");
        assert!(matches!(hata, Hata::Io { .. }));
    }

    #[test]
    fn dizin_yolu_reddedilir() {
        let d = GeciciDizin::yeni("dl-kaynak-dizin").expect("dizin");
        let hata = Kaynak::ac(d.yol(), Ayarlar::default()).expect_err("hata");
        assert!(matches!(hata, Hata::DosyaDegil { .. }));
    }

    #[test]
    fn gecersiz_ayrac_reddedilir() {
        let d = GeciciDizin::yeni("dl-kaynak-ayrac").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a,b\n1,2\n");
        let ayar = Ayarlar {
            ayrac: Some(b'\n'),
            ..Ayarlar::default()
        };
        let hata = Kaynak::ac(&yol, ayar).expect_err("hata");
        assert!(matches!(hata, Hata::GecersizAyrac { .. }));
    }

    #[test]
    fn uzantisiz_dosya_icerikten_cozulur() {
        let d = GeciciDizin::yeni("dl-kaynak-uzantisiz").expect("dizin");
        let yol = yaz(d.yol(), "veri.dat", b"{\"a\":1}\n{\"a\":2}\n");
        let k = ac(&yol);
        assert_eq!(k.bilgi().bicim, "jsonl");
    }

    #[test]
    fn bilinmeyen_bicim_hata_verir() {
        let d = GeciciDizin::yeni("dl-kaynak-bilinmez").expect("dizin");
        let yol = yaz(d.yol(), "veri.dat", b"   \n\t\n");
        let hata = Kaynak::ac(&yol, Ayarlar::default()).expect_err("hata");
        assert!(matches!(hata, Hata::BilinmeyenBicim { .. }));
    }

    #[test]
    fn atlanan_bos_satir_sayisi_bildirilir() {
        let d = GeciciDizin::yeni("dl-kaynak-atlanan").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a\n1\n\n2\n\n");
        let k = ac(&yol);
        assert_eq!(k.satir_sayisi(), 2);
        assert_eq!(k.atlanan_bos_satir(), 2);
    }

    #[test]
    fn bos_dosya_acilir_sutun_yoktur() {
        let d = GeciciDizin::yeni("dl-kaynak-bosdosya").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"");
        let k = ac(&yol);
        assert_eq!(k.satir_sayisi(), 0);
        assert!(k.basliklar().is_empty());
    }
}
