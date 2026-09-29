//! Sabit bellek iddiasının doğrulanması.
//!
//! Kartın ana vaadi: "dosya belleğe alınmaz, yalnızca istenen satır okunur".
//! Bu iddia iki bağımsız testle ölçülür:
//!
//! 1. **Sayacla ölçüm:** aynı satır yapısına sahip 1 MiB'lik ve 64 MiB'lik
//!    dosyalar açılır, her birinden son satır okunur. Okunan bayt sayısı
//!    dosya boyutundan bağımsızsa sabit bellek kanıtlanmış olur.
//! 2. **Kaynak denetimi:** üretim kodunda `read_to_string` / `read_to_end`
//!    çağrısı bulunmadığı doğrulanır. Bu, kuralın ihlal edilmesini derleme
//!    düzeyinde değil, test düzeyinde yakalar.
//!
//! Birim testleri de `#[cfg(test)]` modüllerini tarama dışı bırakır; test
//! kodunun küçük dosyaları okuması sorun değildir.

#[path = "yardimci/mod.rs"]
mod yardimci;

use std::io::Write;
use std::path::Path;

use datalens::indeks::IndeksKipi;
use datalens::kaynak::{Ayarlar, Kaynak};

use yardimci::GeciciDizin;

/// Hedef bayt sayısına ulaşana dek CSV üretir ve yazılan satır sayısını döndürür.
fn csv_uret(yol: &Path, hedef_bayt: u64) -> u64 {
    let dosya = std::fs::File::create(yol).expect("olustur");
    let mut yazici = std::io::BufWriter::new(dosya);
    writeln!(yazici, "no,ad,not,deger").expect("yaz");
    let mut yazilan: u64 = 0;
    let mut no: u64 = 0;
    while yazilan < hedef_bayt {
        let satir = format!("{no},kullanici_{no},not-{no},{no}\n");
        yazici.write_all(satir.as_bytes()).expect("yaz");
        yazilan += satir.len() as u64;
        no += 1;
    }
    yazici.flush().expect("kapat");
    no
}

fn ayar(kip: IndeksKipi) -> Ayarlar {
    Ayarlar {
        indeks: kip,
        ..Ayarlar::default()
    }
}

#[test]
fn son_satir_okundugunda_okunan_bayt_dosya_boyutundan_bagimsizdir() {
    let d = GeciciDizin::yeni("dl-sabit-bellek").expect("dizin");
    let kucuk = d.yol_birestir("kucuk.csv");
    let buyuk = d.yol_birestir("buyuk.csv");
    let kucuk_satir = csv_uret(&kucuk, 1024 * 1024);
    let buyuk_satir = csv_uret(&buyuk, 64 * 1024 * 1024);
    let kucuk_bayt = std::fs::metadata(&kucuk).expect("meta").len();
    let buyuk_bayt = std::fs::metadata(&buyuk).expect("meta").len();
    assert!(
        buyuk_bayt > 60 * 1024 * 1024,
        "buyuk dosya beklenenden kucuk"
    );
    assert!(kucuk_bayt < 2 * 1024 * 1024);
    assert!(
        buyuk_bayt / kucuk_bayt > 30,
        "dosya boyutu orani beklenenden kucuk"
    );

    for kip in [IndeksKipi::Yogun, IndeksKipi::Seyrek] {
        // Tek satır okumanın maliyetini **yalnızca** veri geçişine indirgemek
        // icin acilis sayaclari (baslik cozumleme) okumadan once alinir.
        let mut k1 = Kaynak::ac(&kucuk, ayar(kip)).expect("ac");
        let ilk = k1.istatistik().veri_bayt;
        let son1 = k1.satir(kucuk_satir - 1).expect("satir");
        let kucuk_tek = k1.istatistik().veri_bayt - ilk;

        let mut k2 = Kaynak::ac(&buyuk, ayar(kip)).expect("ac");
        let ilk2 = k2.istatistik().veri_bayt;
        let son2 = k2.satir(buyuk_satir - 1).expect("satir");
        let buyuk_tek = k2.istatistik().veri_bayt - ilk2;

        assert_eq!(son1.no, kucuk_satir - 1);
        assert_eq!(son2.no, buyuk_satir - 1);

        // Maliyet dosya boyutuyla degismez: 64 kat buyuk dosya icin de
        // en fazla kucuk dosyadakinin bir kac kati olmalidir.
        let tavan = (kucuk_tek * 4).max(256 * 1024);
        assert!(
            buyuk_tek <= tavan,
            "kip {kip:?}: 1 MiB dosyada tek satir {kucuk_tek} bayt, \
             64 MiB dosyada {buyuk_tek} bayt okundu (tavan {tavan})"
        );
        assert!(
            buyuk_tek * 1000 < buyuk_bayt,
            "kip {kip:?}: tek satir icin {buyuk_tek} bayt okundu, bu dosya \
             boyutunun %0.1'inden fazla"
        );
        assert!(
            kucuk_tek <= 256 * 1024,
            "kip {kip:?}: 1 MiB dosyada tek satir icin {kucuk_tek} bayt okundu"
        );
    }
}

#[test]
fn sanal_kaydirma_penceresi_dosya_boyutundan_bagimsiz_okur() {
    let d = GeciciDizin::yeni("dl-sabit-pencere").expect("dizin");
    let kucuk = d.yol_birestir("kucuk.csv");
    let buyuk = d.yol_birestir("buyuk.csv");
    let k_satir = csv_uret(&kucuk, 512 * 1024);
    let b_satir = csv_uret(&buyuk, 48 * 1024 * 1024);
    let b_bayt = std::fs::metadata(&buyuk).expect("meta").len();

    let mut k1 = Kaynak::ac(&kucuk, ayar(IndeksKipi::Seyrek)).expect("ac");
    let ilk = k1.istatistik().veri_bayt;
    let p1 = k1.pencere(k_satir - 1, 1).expect("pencere1");
    let kucuk_pencere = k1.istatistik().veri_bayt - ilk;

    let mut k2 = Kaynak::ac(&buyuk, ayar(IndeksKipi::Seyrek)).expect("ac");
    let ilk2 = k2.istatistik().veri_bayt;
    let p2 = k2.pencere(b_satir - 1, 1).expect("pencere2");
    let buyuk_pencere = k2.istatistik().veri_bayt - ilk2;

    assert_eq!(p1[0].no, k_satir - 1);
    assert_eq!(p2[0].no, b_satir - 1);
    assert_eq!(p1[0].alanlar.len(), 4);
    assert_eq!(p2[0].alanlar.len(), 4);

    // 100 kat buyuk dosyada pencere maliyeti pratikte ayni kalir.
    assert!(
        buyuk_pencere <= kucuk_pencere.max(256 * 1024),
        "kucuk pencerede {kucuk_pencere} bayt, buyuk pencerede {buyuk_pencere} bayt"
    );
    assert!(
        buyuk_pencere * 1000 < b_bayt,
        "48 MiB dosyada pencere {buyuk_pencere} bayt okudu, dosya boyutunun \
         %0.1'inden fazla"
    );
}

#[test]
fn indeks_bellek_tutari_dosya_boyutuyla_artis_gostermez() {
    let d = GeciciDizin::yeni("dl-sabit-indeks").expect("dizin");
    let yol = d.yol_birestir("buyuk.csv");
    let satir = csv_uret(&yol, 32 * 1024 * 1024);
    let dosya_bayt = std::fs::metadata(&yol).expect("meta").len();

    let k = Kaynak::ac(&yol, ayar(IndeksKipi::Seyrek)).expect("ac");
    let indeks_bayt = k.bilgi().indeks_bellek_bayt;

    // Seyrek indeks bellek tutari veri boyutunun %1'inden kucuk olmali.
    assert!(
        indeks_bayt * 100 < dosya_bayt,
        "indeks {indeks_bayt} bayt, dosya {dosya_bayt} bayt ({satir} satir)"
    );
    assert_eq!(k.satir_sayisi(), satir);
}

#[test]
fn uretim_kodu_dosyayi_tam_okumaz() {
    let kok = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let yasaklar = [
        "read_to_string",
        "read_to_end",
        "fs::read(",
        "read_to_string_lossy",
    ];
    let mut sayilan = 0usize;
    let mut dosyalar: Vec<std::path::PathBuf> = std::fs::read_dir(&kok)
        .expect("src dizini")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rs"))
        .collect();
    dosyalar.sort();
    assert!(!dosyalar.is_empty(), "src altinda .rs dosyasi yok");

    for yol in &dosyalar {
        let metin = std::fs::read_to_string(yol).expect("kaynak oku");
        // Yalnizca uretim kodu denetlenir; test modulleri haricidir.
        let uretim = match metin.find("#[cfg(test)]") {
            Some(i) => &metin[..i],
            None => &metin[..],
        };
        sayilan += 1;
        for yasak in yasaklar {
            assert!(
                !uretim.contains(yasak),
                "{} icinde yasak cagri bulundu: {yasak}",
                yol.display()
            );
        }
    }
    assert!(sayilan >= 8, "beklenenden az dosya tarandi: {sayilan}");
}
