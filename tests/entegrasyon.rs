//! Kütüphane düzeyinde uçtan uca entegrasyon testleri.
//!
//! Bu dosya "kullanıcı senaryolarını" doğrular: biçim algılama, sütun çözümleme,
//! tip çıkarımı, filtreleme ve dışa aktarım zinciri.

#[path = "yardimci/mod.rs"]
mod yardimci;

use std::io::Write;
use std::path::Path;

use datalens::disa::{disa_aktar, DisaBiimi};
use datalens::filtre::FiltreKumesi;
use datalens::gorunum::{tablo_yaz, GorunumAyar};
use datalens::indeks::IndeksKipi;
use datalens::kaynak::{Ayarlar, Kaynak};
use datalens::profil::profil_uret;
use datalens::tip::SutunTuru;

use yardimci::GeciciDizin;

fn yaz(yol: &Path, icerik: &[u8]) {
    let mut f = std::fs::File::create(yol).expect("olustur");
    f.write_all(icerik).expect("yaz");
    f.flush().expect("kapat");
}

fn ac(yol: &Path) -> Kaynak {
    Kaynak::ac(yol, Ayarlar::default()).expect("ac")
}

/// CSV satırlarını konsol benzeri tabloya basar.
fn bas(kaynak: &Kaynak, satirlar: &[datalens::kaynak::Satir]) -> String {
    let mut tampon: Vec<u8> = Vec::new();
    let ayar = GorunumAyar::default();
    tablo_yaz(&mut tampon, kaynak.basliklar(), &[], satirlar, &ayar).expect("yaz");
    String::from_utf8(tampon).expect("utf8")
}

#[test]
fn csv_dosyasi_uyctan_uca_okunur() {
    let d = GeciciDizin::yeni("dl-int-csv").expect("dizin");
    let yol = d.yol_birestir("veri.csv");
    yaz(
        &yol,
        b"no,ad,yas\n1,ali,30\n2,ayse,41\n3,\"veli, kardes\",22\n4,,35\n",
    );
    let mut k = ac(&yol);
    assert_eq!(k.basliklar(), &["no", "ad", "yas"]);
    assert_eq!(k.satir_sayisi(), 4);

    let satirlar = k.pencere(0, 4).expect("pencere");
    assert_eq!(satirlar[0].alanlar, vec!["1", "ali", "30"]);
    assert_eq!(satirlar[2].alanlar, vec!["3", "veli, kardes", "22"]);
    assert_eq!(satirlar[3].alanlar, vec!["4", "", "35"]);

    let metin = bas(&k, &satirlar);
    assert!(metin.contains("ali"));
    assert!(metin.contains("veli, kardes"));
}

#[test]
fn tirnakli_cok_satirli_alan_komut_sirasinda_kalir() {
    let d = GeciciDizin::yeni("dl-int-cok satir").expect("dizin");
    let yol = d.yol_birestir("veri.csv");
    yaz(&yol, b"a,b\n\" satir1\nsatir2 \",2\n3,4\n");
    let mut k = ac(&yol);
    assert_eq!(k.satir_sayisi(), 2);
    assert_eq!(k.satir(0).unwrap().alanlar, vec![" satir1\nsatir2 ", "2"]);
    assert_eq!(k.satir(1).unwrap().alanlar, vec!["3", "4"]);
}

#[test]
fn tsv_dosyasi_sekme_ayraciyla_profilenir() {
    let d = GeciciDizin::yeni("dl-int-tsv").expect("dizin");
    let yol = d.yol_birestir("veri.tsv");
    yaz(&yol, b"ad\tsayi\nali\t1\nayse\t2\n");
    let mut k = ac(&yol);
    let satirlar = k.pencere(0, 2).expect("pencere");
    let p = profil_uret(k.bilgi(), k.basliklar(), &satirlar, k.satir_sayisi(), 0);
    assert_eq!(p.dosya.bicim, "tsv");
    assert_eq!(p.sutunlar[1].tur, SutunTuru::TamSayi);
    assert_eq!(p.sutunlar[1].sayisal.as_ref().unwrap().max, 2.0);
}

#[test]
fn jsonl_dosyasi_anahtarlardan_sema_uretilir() {
    let d = GeciciDizin::yeni("dl-int-jsonl").expect("dizin");
    let yol = d.yol_birestir("veri.jsonl");
    yaz(
        &yol,
        b"{\"ad\":\"ali\",\"yas\":30}\n{\"ad\":\"ayse\",\"yas\":41}\n",
    );
    let mut k = ac(&yol);
    assert_eq!(k.basliklar(), &["ad", "yas"]);
    let satirlar = k.pencere(0, 2).expect("pencere");
    let p = profil_uret(k.bilgi(), k.basliklar(), &satirlar, k.satir_sayisi(), 0);
    assert_eq!(p.sutunlar[0].tur, SutunTuru::Metin);
    assert_eq!(p.sutunlar[1].tur, SutunTuru::TamSayi);
    assert_eq!(p.sutunlar[1].sayisal.as_ref().unwrap().ortalama, 35.5);
}

#[test]
fn jsonl_dizi_koku_ve_bozuk_satir_birlikte_calisir() {
    let d = GeciciDizin::yeni("dl-int-dizi").expect("dizin");
    let yol = d.yol_birestir("veri.json");
    yaz(&yol, b"[{\"a\":1},{\"a\":2},{\"a\":3}]");
    let mut k = ac(&yol);
    assert!(k.bilgi().dizi_kok);
    assert_eq!(k.satir_sayisi(), 3);
    assert_eq!(k.satir(2).unwrap().alanlar, vec!["3"]);

    let bozuk_yol = d.yol_birestir("bozuk.jsonl");
    yaz(&bozuk_yol, b"{\"a\":1}\n{\"a\": }\n{\"a\":3}\n");
    let mut k2 = ac(&bozuk_yol);
    assert_eq!(k2.satir_sayisi(), 3);
    let satirlar = k2.pencere(0, 3).expect("pencere");
    assert!(satirlar[1].uyari.is_some());
    let metin = bas(&k2, &satirlar);
    assert!(metin.contains('!'), "uyari isareti gosterilmeli");
}

#[test]
fn filtre_uygulanmis_satir_sayisi_dogru_dur() {
    let d = GeciciDizin::yeni("dl-int-filtre").expect("dizin");
    let yol = d.yol_birestir("veri.csv");
    let mut govde = String::from("no,ad\n");
    for i in 0..100 {
        govde.push_str(&format!("{i},k{i}\n"));
    }
    yaz(&yol, govde.as_bytes());
    let mut k = ac(&yol);
    let basliklar = k.basliklar().to_vec();
    let kume = FiltreKumesi::derle(&["no>90".to_string()], &basliklar).expect("derle");
    let eslesen = kume.uygula(k.satir_sayisi(), |no| k.satir(no).unwrap());
    assert_eq!(eslesen.len(), 9);
    assert_eq!(eslesen[0], 91);
    assert_eq!(*eslesen.last().unwrap(), 99);
}

#[test]
fn bilesik_filtre_satirlari_hizlar() {
    let d = GeciciDizin::yeni("dl-int-bilesik").expect("dizin");
    let yol = d.yol_birestir("veri.csv");
    yaz(
        &yol,
        b"sehir,yil\nIstanbul,2020\nAnkara,2021\nIzmir,2022\nIstanbul,2023\n",
    );
    let mut k = ac(&yol);
    let basliklar = k.basliklar().to_vec();
    let kume = FiltreKumesi::derle(
        &["sehir contains stan".to_string(), "yil>2020".to_string()],
        &basliklar,
    )
    .expect("derle");
    let eslesen = kume.uygula(k.satir_sayisi(), |no| k.satir(no).unwrap());
    assert_eq!(eslesen, vec![3]);
}

#[test]
fn filtre_disa_aktarim_gidis_donus_korur() {
    let d = GeciciDizin::yeni("dl-int-disa").expect("dizin");
    let yol = d.yol_birestir("veri.csv");
    yaz(
        &yol,
        b"no,ad,not\n1,ali,\"ilk, okul\"\n2,ayse,de\"diye\n3,veli,sade\n",
    );
    let mut k = ac(&yol);
    let basliklar = k.basliklar().to_vec();
    let kume = FiltreKumesi::derle(&["no>1".to_string()], &basliklar).expect("derle");
    let secilen = kume.uygula(k.satir_sayisi(), |no| k.satir(no).unwrap());
    let satirlar: Vec<_> = secilen.iter().filter_map(|n| k.satir(*n).ok()).collect();

    let hedef = d.yol_birestir("cikti.csv");
    let sonuc = disa_aktar(&hedef, DisaBiimi::Csv, &basliklar, &satirlar).expect("yaz");
    assert_eq!(sonuc.satir, 2);

    let mut k2 = ac(&hedef);
    assert_eq!(k2.basliklar(), &basliklar);
    assert_eq!(k2.satir_sayisi(), 2);
    let tekrar = k2.pencere(0, 2).expect("pencere");
    assert_eq!(tekrar[0].alanlar, vec!["2", "ayse", "de\"diye"]);
    assert_eq!(tekrar[1].alanlar, vec!["3", "veli", "sade"]);
    assert!(tekrar.iter().all(|s| s.uyari.is_none()));
}

#[test]
fn jsonl_disa_aktarim_tipleri_korur() {
    let d = GeciciDizin::yeni("dl-int-disa-jsonl").expect("dizin");
    let yol = d.yol_birestir("veri.jsonl");
    yaz(&yol, b"{\"n\":1,\"o\":true}\n{\"n\":2,\"o\":false}\n");
    let mut k = ac(&yol);
    let basliklar = k.basliklar().to_vec();
    let satirlar = k.pencere(0, 2).expect("pencere");
    let hedef = d.yol_birestir("cikti.jsonl");
    disa_aktar(&hedef, DisaBiimi::Jsonl, &basliklar, &satirlar).expect("yaz");
    let metin = std::fs::read_to_string(&hedef).expect("oku");
    assert!(metin.contains("\"n\":1"));
    assert!(metin.contains("\"o\":true"));

    let mut k2 = ac(&hedef);
    let tekrar = k2.pencere(0, 2).expect("pencere");
    assert_eq!(tekrar[0].alanlar, satirlar[0].alanlar);
}

#[test]
fn tum_bicimler_ayni_sozlukle_acilir() {
    let d = GeciciDizin::yeni("dl-int-bicimler").expect("dizin");
    let csv = d.yol_birestir("a.csv");
    yaz(&csv, b"ad,yas\nali,30\n");
    let tsv = d.yol_birestir("a.tsv");
    yaz(&tsv, b"ad\tyas\nali\t30\n");
    let jsonl = d.yol_birestir("a.jsonl");
    yaz(&jsonl, b"{\"ad\":\"ali\",\"yas\":30}\n");
    let boru = d.yol_birestir("a.csv");
    yaz(&boru, b"ad|yas\nali|30\n");
    let noktali = d.yol_birestir("b.csv");
    yaz(&noktali, b"ad;yas\nali;30\n");

    for (yol, ayrac) in [
        (&csv, ","),
        (&tsv, "\\t"),
        (&jsonl, ""),
        (&boru, "|"),
        (&noktali, ";"),
    ] {
        let k = ac(yol);
        assert_eq!(k.basliklar(), &["ad", "yas"], "ayrac {ayrac} dosya {yol:?}");
        assert_eq!(k.satir_sayisi(), 1);
    }
}

#[test]
fn iki_indeks_kipi_ayni_sonuc_verir() {
    let d = GeciciDizin::yeni("dl-int-kip").expect("dizin");
    let yol = d.yol_birestir("veri.csv");
    let mut govde = String::from("no,ad\n");
    for i in 0..3000 {
        govde.push_str(&format!("{i},satir_{i}\n"));
    }
    yaz(&yol, govde.as_bytes());

    let yogun = Ayarlar {
        indeks: IndeksKipi::Yogun,
        ..Ayarlar::default()
    };
    let mut k1 = Kaynak::ac(&yol, yogun).expect("ac");
    let mut k2 = Kaynak::ac(&yol, Ayarlar::default()).expect("ac");
    assert_eq!(k1.satir_sayisi(), k2.satir_sayisi());
    for no in [0u64, 1, 511, 512, 2999] {
        let a = k1.satir(no).expect("yogun");
        let b = k2.satir(no).expect("seyrek");
        assert_eq!(a.alanlar, b.alanlar, "satir {no}");
    }
}

#[test]
fn oznemli_ayrac_elle_verilebilir() {
    let d = GeciciDizin::yeni("dl-int-ayrac").expect("dizin");
    let yol = d.yol_birestir("veri.dat");
    yaz(&yol, b"a|b\n1|2\n");
    let ayar = Ayarlar {
        bicim: Some(datalens::bicim::Bicim::Csv),
        ayrac: Some(b'|'),
        ..Ayarlar::default()
    };
    let k = Kaynak::ac(&yol, ayar).expect("ac");
    assert_eq!(k.basliklar(), &["a", "b"]);
}

#[test]
fn basliksiz_mod_sutun_adi_uretir() {
    let d = GeciciDizin::yeni("dl-int-basliksiz").expect("dizin");
    let yol = d.yol_birestir("veri.csv");
    yaz(&yol, b"1,2\n3,4\n");
    let ayar = Ayarlar {
        baslik: Some(false),
        ..Ayarlar::default()
    };
    let mut k = Kaynak::ac(&yol, ayar).expect("ac");
    assert_eq!(k.basliklar(), &["sutun_1", "sutun_2"]);
    assert_eq!(k.satir_sayisi(), 2);
    assert_eq!(k.satir(0).unwrap().alanlar, vec!["1", "2"]);
}
