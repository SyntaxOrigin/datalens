//! Bayt aralıklı satır indeksi ve konum bazlı (seek) okuyucu.
//!
//! Bu modül raporun (b05, b07) çekirdek mimarisini taşır: dosya **iki geçişte**
//! okunur.
//!
//! * **Birinci geçiş (indizleme):** dosya baştan sona, sabit boyutlu bir tamponla
//!   akış hâlinde taranır. Her kaydın `(baslangic_bayt, uzunluk)` aralığı
//!   kaydedilir. Tampon boyutu sabittir; dosya boyutundan bağımsızdır.
//! * **İkinci geçiş (veri):** yalnızca istenen satırlar `File::seek` ile konum
//!   bazlı okunur. Dosya **hiçbir zaman** belleğe tümüyle alınmaz.
//!
//! İki indeks kipi vardır:
//!
//! | Kip | Bellek | Rastgele erişim |
//! |---|---|---|
//! | [`IndeksKipi::Yogun`] | her satır için 16 bayt | tek `seek` + tek `read` |
//! | [`IndeksKipi::Seyrek`] | her blok için 8 bayt | `seek` + blok içi ileri tarama |
//!
//! Seyrek kip, kartın "milyar satırlık dosyada indeks tek başına bütçeyi
//! aşabilir" riskine (MANIFEST Kart 26) doğrudan karşılık verir: indeks bellek
//! tüketimi dosya boyutundan değil, **blok sayısından** artar.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

use crate::ayristirici::{kayit_tara, KayitCozum, Tarama};
use crate::bicim::Bicim;
use crate::hata::{io_hata, Hata};

/// Bir kaydın dosya içindeki bayt aralığı.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct KayitAraligi {
    /// İçeriğin dosya başından itibaren bayt ofseti.
    pub baslangic: u64,
    /// İçeriğin bayt uzunluğu (satır sonu dahil değildir).
    pub uzunluk: u64,
}

/// Satır indeksinin tutulma biçimi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndeksKipi {
    /// Her satırın tam aralığı saklanır. En hızlı rastgele erişim, satır sayısıyla
    /// artan bellek.
    Yogun,
    /// Yalnızca blok başları saklanır; blok içi satırlar ileri taranarak bulunur.
    /// Bellek dosya boyutundan bağımsızdır, rastgele erişim bir blok taraması
    /// kadar ek yol okuması gerektirir.
    Seyrek,
}

impl IndeksKipi {
    /// Kipin adı; JSON çıktısında kullanılır.
    pub fn ad(self) -> &'static str {
        match self {
            IndeksKipi::Yogun => "yogun",
            IndeksKipi::Seyrek => "seyrek",
        }
    }
}

/// İndeksleme ve okuma için kullanılan tampon boyutları.
#[derive(Debug, Clone, Copy)]
pub struct Sablon {
    /// Birinci geçişte (indizleme) kullanılan tampon boyutu.
    pub indeks_tampon: usize,
    /// İkinci geçişte (veri okuma) kullanılan tampon boyutu.
    pub veri_tampon: usize,
    /// Seyrek kipte bir blokta tutulan satır sayısı.
    pub blok_satir: usize,
}

impl Default for Sablon {
    fn default() -> Self {
        Sablon {
            indeks_tampon: 256 * 1024,
            veri_tampon: 16 * 1024,
            blok_satir: 512,
        }
    }
}

/// Kaydırılabilir, sabit boyutlu tampon üzerinden okuyan dosya okuyucu.
///
/// `std::io::BufReader` bilinçli olarak **kullanılmaz**: `BufReader` ileri okumaya
/// göre optimize edildiği için konum bazlı `seek` + `read_exact` kalıbıyla
/// gereksiz bayt okuması yapar. Buradaki okuyucu yalnızca istenen pencereyi
/// doldurur; kayıt tamponun sınırını aştığında tamponu kaydırır, gerekirse
/// büyütür.
pub(crate) struct Kaydirma<R> {
    dosya: R,
    tampon: Vec<u8>,
    /// `tampon[0]` baytının dosya içindeki ofseti.
    taban: u64,
    /// Tampta geçerli olan bayt sayısı.
    uzunluk: usize,
    /// Okuyucunun içinde bulunduğu tampon konumu.
    konum: usize,
    /// Dosya sonuna ulaşıldı mı?
    eof: bool,
    /// Okunan toplam bayt (istatistik için).
    okunan: u64,
    /// Yapılan toplam `seek` sayısı (istatistik için).
    seek: u64,
    /// Okuma yolu (hata mesajı için).
    yol: std::path::PathBuf,
}

impl<R: Read + Seek> Kaydirma<R> {
    /// Verilen boyutta boş bir okuyucu oluşturur; henüz okuma yapmaz.
    pub(crate) fn ac(dosya: R, tampon_boyutu: usize, yol: &Path) -> Self {
        let boyut = tampon_boyutu.max(1024);
        Kaydirma {
            dosya,
            tampon: vec![0u8; boyut],
            taban: 0,
            uzunluk: 0,
            konum: 0,
            eof: false,
            okunan: 0,
            seek: 0,
            yol: yol.to_path_buf(),
        }
    }

    /// Okuyucuyu mutlak `bayt` konumuna taşır ve tamponu boşaltır.
    pub(crate) fn konumlan(&mut self, bayt: u64) -> io::Result<()> {
        self.dosya.seek(SeekFrom::Start(bayt))?;
        self.seek = self.seek.saturating_add(1);
        self.taban = bayt;
        self.uzunluk = 0;
        self.konum = 0;
        self.eof = false;
        Ok(())
    }

    /// Tampon içi `hedef` konumundan itibaren veri olduğundan emin olur ve
    /// okuyucunun konumunu oraya taşır.
    ///
    /// Tampon yetmiyorsa yalnızca **kaydırılır**; tamponun boyutu hiçbir zaman
    /// büyütülmez. Büyütme yetkisi yalnızca [`Kaydirma::uzat`]'tedir ve o da
    /// yalnızca *tek bir kayıt* tampona sığmadığında devreye girer. Aksi hâlde
    /// indeksleme sırasında tampon dosya boyutuna doğru büyür ve "sabit bellek"
    /// vaadi bozulur.
    ///
    /// `hedef` tampon içi indeks olduğu için kaydırmadan sonra yeniden
    /// güncellenir; aksi hâlde istenen konum kadar fazla ilerlenir ve kayıtlar
    /// atlanır.
    pub(crate) fn doldur(&mut self, hedef: usize) -> io::Result<()> {
        let mut hedef = hedef;
        loop {
            if hedef < self.uzunluk {
                self.konum = hedef;
                return Ok(());
            }
            if self.eof {
                self.konum = self.uzunluk;
                return Ok(());
            }
            // Tampon tam doluysa hedef'ten onceki veri kaydirilir. Tamponun bos
            // alani varsa hicbir sey kaydirilmaz.
            if self.uzunluk == self.tampon.len() {
                let atilacak = hedef.min(self.uzunluk);
                if atilacak > 0 {
                    self.tampon.copy_within(atilacak..self.uzunluk, 0);
                    self.uzunluk -= atilacak;
                    self.taban = self.taban.saturating_add(atilacak as u64);
                    hedef -= atilacak;
                }
            }
            let baslanan = self.uzunluk;
            let n = self.dosya.read(&mut self.tampon[baslanan..])?;
            if n == 0 {
                self.eof = true;
            } else {
                self.uzunlak_ekle(n);
            }
        }
    }

    /// Tampon dolduğunda mevcut konumdan itibaren **kaydırır** ve yeni baytlar
    /// okur.
    ///
    /// Konum sıfırdan büyükse tüketilen veri atılır; konum zaten sıfırsa tek bir
    /// kayıt tampona sığmıyor demektir ve tampon iki katına büyütülür. Büyüme
    /// yalnızca bu son durumda olur.
    pub(crate) fn kaydir_ve_doldur(&mut self) -> io::Result<()> {
        if self.konum > 0 {
            let atilacak = self.konum.min(self.uzunluk);
            self.tampon.copy_within(atilacak..self.uzunluk, 0);
            self.uzunluk -= atilacak;
            self.taban = self.taban.saturating_add(atilacak as u64);
            self.konum = 0;
        } else {
            let yeni = self.tampon.len().saturating_mul(2);
            self.tampon.resize(yeni, 0);
        }
        let baslanan = self.uzunluk;
        let n = self.dosya.read(&mut self.tampon[baslanan..])?;
        if n == 0 {
            self.eof = true;
        } else {
            self.uzunlak_ekle(n);
        }
        Ok(())
    }

    /// Okuyucunun iç tampon boyutu (bayt).
    ///
    /// `uyat()` yalnızca tek bir kayıt tampona sığmadığında bu değeri artırır.
    pub(crate) fn tampon_boyutu(&self) -> usize {
        self.tampon.len()
    }

    /// Tamponun **sonuna** yeni baytlar ekler; mevcut veriyi kaydırmaz veya
    /// silmez.
    ///
    /// Tampon tamamen doluysa iki katına çıkarılır. Bu yalnızca **tek bir kayıt**
    /// tampona sığmadığında olur; normal tarama bu yönteme hiç ihtiyaç duymaz.
    pub(crate) fn uzat(&mut self) -> io::Result<()> {
        if self.uzunluk == self.tampon.len() {
            let yeni = self.tampon.len().saturating_mul(2);
            self.tampon.resize(yeni, 0);
        }
        let baslanan = self.uzunluk;
        let n = self.dosya.read(&mut self.tampon[baslanan..])?;
        if n == 0 {
            self.eof = true;
        } else {
            self.uzunlak_ekle(n);
        }
        Ok(())
    }

    /// Okunan bayt sayacını ilerletir.
    fn uzunlak_ekle(&mut self, n: usize) {
        self.uzunluk += n;
        self.okunan = self.okunan.saturating_add(n as u64);
    }

    /// Tamponun geçerli kısmı.
    pub(crate) fn baytlar(&self) -> &[u8] {
        &self.tampon[..self.uzunluk]
    }

    /// Tamponun geçerli kısmının uzunluğu.
    pub(crate) fn uzunluk(&self) -> usize {
        self.uzunluk
    }

    /// Okuyucunun bulunduğu tampon konumu.
    pub(crate) fn tampon_konumu(&self) -> usize {
        self.konum
    }

    /// Tampon içi bir konumu mutlak ofsete çevirir.
    pub(crate) fn taban_ofset(&self, konum: usize) -> u64 {
        self.taban.saturating_add(konum as u64)
    }

    /// Dosya sonuna ulaşıldı mı?
    pub(crate) fn eof(&self) -> bool {
        self.eof
    }

    /// Okunan bayt ve `seek` sayısı.
    pub(crate) fn sayaclar(&self) -> (u64, u64) {
        (self.okunan, self.seek)
    }

    /// Okuma hatasını yol bilgisiyle sarmalar.
    pub(crate) fn hata(&self, islem: &'static str, kaynak: io::Error) -> Hata {
        io_hata(islem, &self.yol, kaynak)
    }
}

/// [`Kaydirma`] üzerinde bulunan kayıt tarayıcısının ürettiği kayıt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Kayit {
    /// Kaydın bayt aralığı.
    pub aralik: KayitAraligi,
    /// Kayıt yalnızca boşluktan mı oluşuyor?
    pub bos: bool,
}

/// Okuyucudan sıradaki kaydı okur ve konumu kaydın sonrasına taşır.
///
/// Tamamen boş satırlar atlanır (JSON dizi kökünde mümkün değildir); çağıran taraf
/// `bos` bayrağını sayacağına geçer. JSON dizi kökü bittiğinde `None` döner.
pub(crate) fn sonraki_kayit(
    okuyucu: &mut Kaydirma<File>,
    cozum: &KayitCozum,
) -> Result<Option<Kayit>, Hata> {
    if cozum.dizi_kok {
        // Elemanlar arasindaki bosluk ve virgulleri atla.
        loop {
            let konum = okuyucu.tampon_konumu();
            okuyucu
                .doldur(konum)
                .map_err(|e| okuyucu.hata("indeksleme", e))?;
            let baytlar = okuyucu.baytlar();
            let k = okuyucu.tampon_konumu();
            if k >= baytlar.len() {
                return Ok(None);
            }
            match baytlar[k] {
                b' ' | b'\t' | b'\r' | b'\n' | b',' => {
                    let sonraki = k + 1;
                    okuyucu
                        .doldur(sonraki)
                        .map_err(|e| okuyucu.hata("indeksleme", e))?;
                }
                _ => break,
            }
        }
    }

    loop {
        let konum = okuyucu.tampon_konumu();
        okuyucu
            .doldur(konum)
            .map_err(|e| okuyucu.hata("indeksleme", e))?;
        // Tamponun sonu ve dosya sonu aynı noktadaysa indeksleme bitti. Bu
        // kontrol olmadan dosya sonunda bos bir kayit uretilir ve dongu
        // surdurulemez.
        if konum >= okuyucu.uzunluk() && okuyucu.eof() {
            return Ok(None);
        }
        let tarama = kayit_tara(okuyucu.baytlar(), konum, cozum, okuyucu.eof());
        match tarama {
            // Tampon kaydi tamamlamaya yetmedi: yer varsa uzat, yoksa kaydir.
            // Kaydirma indeksleme tamponunu buyutmez; boyut yalnizca tek bir
            // kayit tampona sigmiyorsa artar.
            Tarama::Devam => {
                if okuyucu.uzunluk() < okuyucu.tampon_boyutu() {
                    okuyucu.uzat().map_err(|e| okuyucu.hata("indeksleme", e))?;
                } else {
                    okuyucu
                        .kaydir_ve_doldur()
                        .map_err(|e| okuyucu.hata("indeksleme", e))?;
                }
            }
            Tarama::DiziBitti => return Ok(None),
            Tarama::Tamam {
                baslangic,
                son,
                sonraki,
            } => {
                let aralik = KayitAraligi {
                    baslangic: okuyucu.taban_ofset(baslangic),
                    uzunluk: (son.saturating_sub(baslangic)) as u64,
                };
                okuyucu
                    .doldur(sonraki)
                    .map_err(|e| okuyucu.hata("indeksleme", e))?;
                return Ok(Some(Kayit {
                    aralik,
                    bos: aralik.uzunluk == 0,
                }));
            }
        }
    }
}

/// Tamamlanmış satır indeksi.
#[derive(Debug, Clone)]
pub struct SatirIndeksi {
    kip: IndeksKipi,
    araliklar: Vec<KayitAraligi>,
    anklar: Vec<u64>,
    blok_satir: usize,
    satir_sayisi: u64,
    dosya_boyutu: u64,
    atlanan_bos: u64,
    cozum: KayitCozum,
}

impl SatirIndeksi {
    /// Toplam kayıt (satır) sayısı.
    pub fn satir_sayisi(&self) -> u64 {
        self.satir_sayisi
    }

    /// İndekslenirken atlanan tamamen boş satır sayısı.
    ///
    /// Sessizce kaybolan satır olmasın diye profil çıktısında raporlanır.
    pub fn atlanan_bos_satir(&self) -> u64 {
        self.atlanan_bos
    }

    /// Dosyanın boyutu (bayt).
    pub fn dosya_boyutu(&self) -> u64 {
        self.dosya_boyutu
    }

    /// Kullanılan indeks kipi.
    pub fn kip(&self) -> IndeksKipi {
        self.kip
    }

    /// Seyrek kipte bir bloktaki satır sayısı.
    pub fn blok_satir(&self) -> usize {
        self.blok_satir
    }

    /// Dosyanın kökü JSON dizi miydi?
    pub fn dizi_kok(&self) -> bool {
        self.cozum.dizi_kok
    }

    /// Kayıt sınırlayıcısının çözümleme bilgisi.
    pub fn cozum(&self) -> KayitCozum {
        self.cozum
    }

    /// İndeksin yaklaşık bellek tüketimi (bayt).
    ///
    /// Bu değer dosyanın **verisini** değil, yalnızca satır konumlarını kapsar.
    pub fn bellek_tahmini(&self) -> u64 {
        match self.kip {
            IndeksKipi::Yogun => {
                (self.araliklar.len() * std::mem::size_of::<KayitAraligi>()) as u64
            }
            IndeksKipi::Seyrek => (self.anklar.len() * std::mem::size_of::<u64>()) as u64,
        }
    }
}

/// Dosyayı baştan sona tarayarak satır indeksini kurar (birinci geçiş).
pub fn kur(
    dosya: File,
    yol: &Path,
    bicim: Bicim,
    ayrac: u8,
    kip: IndeksKipi,
    sablon: Sablon,
    dosya_boyutu: u64,
) -> Result<SatirIndeksi, Hata> {
    let blok_satir = sablon.blok_satir.max(1);
    let mut okuyucu = Kaydirma::ac(dosya, sablon.indeks_tampon, yol);

    let cozum = cozum_senir(&mut okuyucu, bicim, ayrac)?;
    // Yogun kipte her kaydin tam araligi saklanir. Seyrek kipte yalnizca blok
    // baslari saklanir; boylece indeks bellegi dosya boyutundan degil, blok
    // sayisindan artar.
    let mut araliklar: Vec<KayitAraligi> = Vec::new();
    let mut anklar: Vec<u64> = Vec::new();
    let mut sayac: u64 = 0;
    let mut atlanan = 0u64;

    while let Some(kayit) = sonraki_kayit(&mut okuyucu, &cozum)? {
        if kayit.bos {
            atlanan += 1;
            continue;
        }
        match kip {
            IndeksKipi::Yogun => araliklar.push(kayit.aralik),
            IndeksKipi::Seyrek => {
                if sayac % blok_satir as u64 == 0 {
                    anklar.push(kayit.aralik.baslangic);
                }
            }
        }
        sayac += 1;
    }
    let satir_sayisi = sayac;
    Ok(SatirIndeksi {
        kip,
        araliklar,
        anklar,
        blok_satir,
        satir_sayisi,
        dosya_boyutu,
        atlanan_bos: atlanan,
        cozum,
    })
}

/// Biçim ve dizi kökü bilgisini ilk anlamlı bayta bakarak belirler.
fn cozum_senir(okuyucu: &mut Kaydirma<File>, bicim: Bicim, ayrac: u8) -> Result<KayitCozum, Hata> {
    if bicim != Bicim::Jsonl {
        okuyucu
            .doldur(0)
            .map_err(|e| okuyucu.hata("indeksleme", e))?;
        return Ok(KayitCozum {
            bicim,
            ayrac,
            dizi_kok: false,
        });
    }
    let konum = ilk_anlamli_konum(okuyucu)?;
    let ilk = okuyucu.baytlar().get(konum).copied();
    if ilk == Some(b'[') {
        okuyucu
            .doldur(konum + 1)
            .map_err(|e| okuyucu.hata("indeksleme", e))?;
        Ok(KayitCozum {
            bicim,
            ayrac,
            dizi_kok: true,
        })
    } else {
        okuyucu
            .doldur(0)
            .map_err(|e| okuyucu.hata("indeksleme", e))?;
        Ok(KayitCozum {
            bicim,
            ayrac,
            dizi_kok: false,
        })
    }
}

/// Dosyanın başındaki boşlukları atlayıp ilk anlamlı baytın tampon içi konumunu
/// döner. Dosya tamamen boşsa geçerli uzunluk döner.
fn ilk_anlamli_konum(okuyucu: &mut Kaydirma<File>) -> Result<usize, Hata> {
    let mut hedef = 0usize;
    loop {
        okuyucu
            .doldur(hedef)
            .map_err(|e| okuyucu.hata("indeksleme", e))?;
        let baytlar = okuyucu.baytlar();
        let mut i = okuyucu.tampon_konumu();
        while i < baytlar.len() && baytlar[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < baytlar.len() {
            return Ok(i);
        }
        if okuyucu.eof() {
            return Ok(okuyucu.uzunluk());
        }
        hedef = i;
    }
}

/// `no` numaralı satırın bayt aralığını bulur (ikinci geçiş, veri okuma).
///
/// Yoğun kipte tek dizi erişimi yeterlidir. Seyrek kipte ilgili blok başına
/// konumlanıp blok içindeki satırlar ileri taranır; okunan bayt miktarı
/// dosya boyutundan değil, **blok boyutundan** bağımsızdır.
pub(crate) fn aralik_bul(
    okuyucu: &mut Kaydirma<File>,
    indeks: &SatirIndeksi,
    no: u64,
) -> Result<KayitAraligi, Hata> {
    if no >= indeks.satir_sayisi {
        return Err(Hata::GecersizAralik {
            baslangic: no,
            adet: 1,
            satir_sayisi: indeks.satir_sayisi,
        });
    }
    match indeks.kip {
        IndeksKipi::Yogun => Ok(indeks.araliklar[no as usize]),
        IndeksKipi::Seyrek => {
            let blok_satir = indeks.blok_satir as u64;
            let blok = (no / blok_satir) as usize;
            let hedef = no % blok_satir;
            let baslangic = indeks.anklar[blok];
            okuyucu
                .konumlan(baslangic)
                .map_err(|e| okuyucu.hata("okuma", e))?;
            let cozum = indeks.cozum();
            let mut son = KayitAraligi::default();
            for _ in 0..=hedef {
                match sonraki_kayit(okuyucu, &cozum)? {
                    Some(kayit) => son = kayit.aralik,
                    None => {
                        return Err(Hata::GecersizAralik {
                            baslangic: no,
                            adet: 1,
                            satir_sayisi: indeks.satir_sayisi,
                        })
                    }
                }
            }
            Ok(son)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::test_yardimcisi::GeciciDizin;
    use std::io::Write;

    fn yaz(klasor: &Path, ad: &str, icerik: &[u8]) -> std::path::PathBuf {
        let yol = klasor.join(ad);
        let mut f = File::create(&yol).expect("olustur");
        f.write_all(icerik).expect("yaz");
        f.flush().expect("kapat");
        yol
    }

    fn kur_test(yol: &Path, bicim: Bicim, ayrac: u8, kip: IndeksKipi) -> SatirIndeksi {
        kur_test_sablon(yol, bicim, ayrac, kip, Sablon::default())
    }

    fn kur_test_sablon(
        yol: &Path,
        bicim: Bicim,
        ayrac: u8,
        kip: IndeksKipi,
        sablon: Sablon,
    ) -> SatirIndeksi {
        let dosya = File::open(yol).expect("ac");
        let boyut = dosya.metadata().expect("meta").len();
        kur(dosya, yol, bicim, ayrac, kip, sablon, boyut).expect("indeksle")
    }

    #[test]
    fn indeks_kipi_adlari_yazilir() {
        assert_eq!(IndeksKipi::Yogun.ad(), "yogun");
        assert_eq!(IndeksKipi::Seyrek.ad(), "seyrek");
    }

    #[test]
    fn sablon_varsayilanlari_urun_duruyor() {
        let s = Sablon::default();
        assert_eq!(s.blok_satir, 512);
        assert!(s.indeks_tampon >= 65536);
        assert!(s.veri_tampon >= 4096);
    }

    #[test]
    fn duz_satirlar_sayilir() {
        let d = GeciciDizin::yeni("dl-idx-duz").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a,b\n1,2\n3,4\n5,6\n");
        for kip in [IndeksKipi::Yogun, IndeksKipi::Seyrek] {
            let ix = kur_test(&yol, Bicim::Csv, b',', kip);
            assert_eq!(ix.satir_sayisi(), 4, "kip {kip:?}");
            assert_eq!(ix.atlanan_bos_satir(), 0);
        }
    }

    #[test]
    fn araliklar_artar_ve_kapsayicidir() {
        let d = GeciciDizin::yeni("dl-idx-aralik").expect("dizin");
        let icerik = b"aaa,bb\nc,dddd\ne,ff\n";
        let yol = yaz(d.yol(), "a.csv", icerik);
        let ix = kur_test(&yol, Bicim::Csv, b',', IndeksKipi::Yogun);
        let araliklar = ix.araliklar.clone();
        assert_eq!(araliklar.len(), 3);
        let mut onceki_son = 0u64;
        for (i, a) in araliklar.iter().enumerate() {
            assert!(a.baslangic >= onceki_son, "kayit {i} kesisiyor");
            assert!(
                a.baslangic + a.uzunluk <= icerik.len() as u64,
                "kayit {i} tasiyor"
            );
            onceki_son = a.baslangic + a.uzunluk;
        }
    }

    #[test]
    fn son_satirsiz_sonlandirilmis_dosya_indekslenir() {
        let d = GeciciDizin::yeni("dl-idx-sonsatir").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a,b\n1,2");
        let ix = kur_test(&yol, Bicim::Csv, b',', IndeksKipi::Yogun);
        assert_eq!(ix.satir_sayisi(), 2);
    }

    #[test]
    fn bos_dosya_indekslenir() {
        let d = GeciciDizin::yeni("dl-idx-bos").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"");
        let ix = kur_test(&yol, Bicim::Csv, b',', IndeksKipi::Yogun);
        assert_eq!(ix.satir_sayisi(), 0);
        assert_eq!(ix.dosya_boyutu(), 0);
    }

    #[test]
    fn yalniz_baslik_indekslenir() {
        let d = GeciciDizin::yeni("dl-idx-baslik").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a,b,c");
        let ix = kur_test(&yol, Bicim::Csv, b',', IndeksKipi::Yogun);
        assert_eq!(ix.satir_sayisi(), 1);
    }

    #[test]
    fn bos_satirlar_atlanir_ve_sayilir() {
        let d = GeciciDizin::yeni("dl-idx-bossatir").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a,b\n\n1,2\n\n\n3,4\n");
        let ix = kur_test(&yol, Bicim::Csv, b',', IndeksKipi::Yogun);
        assert_eq!(ix.satir_sayisi(), 3);
        assert_eq!(ix.atlanan_bos_satir(), 3);
    }

    #[test]
    fn tirnak_ici_satir_tek_kayit_sayilir() {
        let d = GeciciDizin::yeni("dl-idx-tirnak").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a,b\n\"x\ny\",2\n3,4\n");
        let ix = kur_test(&yol, Bicim::Csv, b',', IndeksKipi::Yogun);
        assert_eq!(ix.satir_sayisi(), 3);
    }

    #[test]
    fn jsonl_dizi_kok_algilanir() {
        let d = GeciciDizin::yeni("dl-idx-dizi").expect("dizin");
        let yol = yaz(d.yol(), "a.json", br#"[{"a":1},{"a":2},{"a":3}]"#);
        let ix = kur_test(&yol, Bicim::Jsonl, 0, IndeksKipi::Yogun);
        assert!(ix.dizi_kok());
        assert_eq!(ix.satir_sayisi(), 3);
    }

    #[test]
    fn jsonl_dizi_kok_izgara_olmayan_dosyada_kapali() {
        let d = GeciciDizin::yeni("dl-idx-diziyok").expect("dizin");
        let yol = yaz(d.yol(), "a.jsonl", b"{\"a\":1}\n{\"a\":2}\n");
        let ix = kur_test(&yol, Bicim::Jsonl, 0, IndeksKipi::Yogun);
        assert!(!ix.dizi_kok());
        assert_eq!(ix.satir_sayisi(), 2);
    }

    #[test]
    fn bos_jsonl_dizi_sifir_satir_verir() {
        let d = GeciciDizin::yeni("dl-idx-bosdizi").expect("dizin");
        let yol = yaz(d.yol(), "a.json", b"[ ]");
        let ix = kur_test(&yol, Bicim::Jsonl, 0, IndeksKipi::Yogun);
        assert!(ix.dizi_kok());
        assert_eq!(ix.satir_sayisi(), 0);
    }

    #[test]
    fn tsv_ayraci_ayri_tutulur() {
        let d = GeciciDizin::yeni("dl-idx-tsv").expect("dizin");
        let yol = yaz(d.yol(), "a.tsv", b"a\tb\n1\t2\n3\t4\n");
        let ix = kur_test(&yol, Bicim::Tsv, b'\t', IndeksKipi::Yogun);
        assert_eq!(ix.satir_sayisi(), 3);
        assert_eq!(ix.cozum().ayrac, b'\t');
    }

    #[test]
    fn seyrek_indeks_bellek_tahmini_dosya_boyutundan_kucuktur() {
        let d = GeciciDizin::yeni("dl-idx-bellek").expect("dizin");
        let mut govde = String::from("a,b\n");
        for i in 0..5000 {
            govde.push_str(&format!("{i},{i}\n"));
        }
        let yol = yaz(d.yol(), "a.csv", govde.as_bytes());
        let yogun = kur_test(&yol, Bicim::Csv, b',', IndeksKipi::Yogun);
        let seyrek = kur_test(&yol, Bicim::Csv, b',', IndeksKipi::Seyrek);
        assert_eq!(yogun.satir_sayisi(), seyrek.satir_sayisi());
        assert_eq!(yogun.satir_sayisi(), 5001);
        assert!(seyrek.bellek_tahmini().saturating_mul(100) < yogun.bellek_tahmini());
    }

    #[test]
    fn iki_kip_ayni_araligi_dondurur() {
        let d = GeciciDizin::yeni("dl-idx-karsilastir").expect("dizin");
        let mut govde = String::from("no,ad\n");
        for i in 0..2000 {
            govde.push_str(&format!("{i},ad{i}\n"));
        }
        let yol = yaz(d.yol(), "a.csv", govde.as_bytes());
        let baytlar = govde.as_bytes();
        for kip in [IndeksKipi::Yogun, IndeksKipi::Seyrek] {
            let ix = kur_test(&yol, Bicim::Csv, b',', kip);
            let dosya = File::open(&yol).expect("ac");
            let mut oku = Kaydirma::ac(dosya, 4096, &yol);
            for no in [0u64, 1, 7, 511, 512, 1500, ix.satir_sayisi() - 1] {
                let a = aralik_bul(&mut oku, &ix, no).expect("aralik");
                let metin = std::str::from_utf8(
                    &baytlar[a.baslangic as usize..(a.baslangic + a.uzunluk) as usize],
                )
                .expect("utf8");
                if no == 0 {
                    assert_eq!(metin, "no,ad", "kip {kip:?}");
                } else {
                    let veri = no - 1;
                    let beklenen = format!("{veri},ad{veri}");
                    assert_eq!(metin, beklenen, "kip {kip:?} satir {no}");
                }
            }
        }
    }

    #[test]
    fn indeksleme_tamponu_dosya_boyutuyla_buyumez() {
        // Tampon, dosyadan belirgin uzun olmali ki kaydirma yolu calissin.
        let d = GeciciDizin::yeni("dl-idx-tampon").expect("dizin");
        let yol = d.yol().join("buyuk.csv");
        let mut f = File::create(&yol).expect("olustur");
        let mut yazici = std::io::BufWriter::new(&mut f);
        writeln!(yazici, "no,ad").expect("yaz");
        for i in 0..40_000u64 {
            writeln!(yazici, "{i},kullanici_{i}").expect("yaz");
        }
        yazici.flush().expect("kapat");
        drop(yazici);
        let boyut = std::fs::metadata(&yol).expect("meta").len();
        assert!(boyut > 512 * 1024, "test dosyasi beklenenden kucuk");

        let sablon = Sablon {
            indeks_tampon: 64 * 1024,
            veri_tampon: 4096,
            blok_satir: 512,
        };
        let ix = kur_test_sablon(&yol, Bicim::Csv, b',', IndeksKipi::Yogun, sablon);
        assert_eq!(ix.satir_sayisi(), 40_001);

        // Ayni tarama dogrudan yapilir ve tamponun buyumedigi gozlenir.
        let dosya = File::open(&yol).expect("ac");
        let mut oku = Kaydirma::ac(dosya, 64 * 1024, &yol);
        let cozum = KayitCozum {
            bicim: Bicim::Csv,
            ayrac: b',',
            dizi_kok: false,
        };
        let mut sayilan = 0u64;
        while sonraki_kayit(&mut oku, &cozum).expect("tara").is_some() {
            sayilan += 1;
        }
        assert_eq!(sayilan, 40_001);
        assert_eq!(
            oku.tampon_boyutu(),
            64 * 1024,
            "indeksleme sirasinda tampon buyudu"
        );
    }

    #[test]
    fn tamplerin_tamponundan_uzun_kayit_okunur() {
        let d = GeciciDizin::yeni("dl-idx-uzunkayit").expect("dizin");
        let yol = d.yol().join("uzun.csv");
        let mut govde = String::from("a\n");
        govde.push('"');
        govde.push_str(&"x".repeat(5000));
        govde.push_str("\"\n");
        govde.push_str("1,2\n");
        let mut f = File::create(&yol).expect("olustur");
        f.write_all(govde.as_bytes()).expect("yaz");
        f.flush().expect("kapat");
        drop(f);

        let dosya = File::open(&yol).expect("ac");
        let mut oku = Kaydirma::ac(dosya, 1024, &yol);
        let cozum = KayitCozum {
            bicim: Bicim::Csv,
            ayrac: b',',
            dizi_kok: false,
        };
        let ilk = sonraki_kayit(&mut oku, &cozum)
            .expect("tara1")
            .expect("kayit1");
        assert_eq!(ilk.aralik.baslangic, 0);
        assert_eq!(ilk.aralik.uzunluk, 1);
        let ikinci = sonraki_kayit(&mut oku, &cozum)
            .expect("tara2")
            .expect("kayit2");
        assert_eq!(ikinci.aralik.uzunluk, 5002);
        // Tampon buyumek zorundaydi: tek kayit 1024 bayttan buyuk.
        assert!(oku.tampon_boyutu() > 1024);
        let ucuncu = sonraki_kayit(&mut oku, &cozum)
            .expect("tara3")
            .expect("kayit3");
        assert_eq!(ucuncu.aralik.uzunluk, 3);
        assert!(sonraki_kayit(&mut oku, &cozum).expect("tara4").is_none());
    }

    #[test]
    fn kip_disi_satir_hata_verir() {
        let d = GeciciDizin::yeni("dl-idx-disi").expect("dizin");
        let yol = yaz(d.yol(), "a.csv", b"a,b\n1,2\n");
        let ix = kur_test(&yol, Bicim::Csv, b',', IndeksKipi::Yogun);
        let dosya = File::open(&yol).expect("ac");
        let mut oku = Kaydirma::ac(dosya, 4096, &yol);
        let hata = aralik_bul(&mut oku, &ix, 5).expect_err("hata beklenir");
        assert!(matches!(hata, Hata::GecersizAralik { .. }));
    }
}
