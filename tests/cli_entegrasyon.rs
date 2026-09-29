//! Komut satırı arayüzünün uçtan uca testleri.
//!
//! Gerçek derlenmiş ikili çalıştırılır (`CARGO_BIN_EXE_datalens`); böylece
//! `clap` tanımları, çıkış kodu ve metin çıktısı birlikte doğrulanır.

#[path = "yardimci/mod.rs"]
mod yardimci;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};

use yardimci::GeciciDizin;

fn yaz(yol: &Path, icerik: &[u8]) {
    let mut f = std::fs::File::create(yol).expect("olustur");
    f.write_all(icerik).expect("yaz");
    f.flush().expect("kapat");
}

fn calistir(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datalens"))
        .args(args)
        .output()
        .expect("ikili calistir")
}

fn basarili(args: &[&str]) -> String {
    let cikti = calistir(args);
    assert!(
        cikti.status.success(),
        "komut basarisiz: {args:?}\n{}",
        String::from_utf8_lossy(&cikti.stderr)
    );
    String::from_utf8_lossy(&cikti.stdout).into_owned()
}

fn ornek_veri(dizin: &GeciciDizin) -> std::path::PathBuf {
    let yol = dizin.yol_birestir("veri.csv");
    yaz(
        &yol,
        b"no,ad,yas\n1,ali,30\n2,ayse,41\n3,veli,22\n4,ece,35\n5,can,28\n",
    );
    yol
}

#[test]
fn yardim_ciktisi_uygulama_adini_gosterir() {
    let metin = basarili(&["--help"]);
    assert!(metin.contains("datalens"));
    assert!(metin.contains("schema"));
    assert!(metin.contains("export"));
}

#[test]
fn surum_bilgisi_yazilir() {
    let metin = basarili(&["--version"]);
    assert!(metin.contains("datalens"));
    assert!(metin.contains("0.1.0"));
}

#[test]
fn schema_alt_komutu_json_yazar() {
    let d = GeciciDizin::yeni("dl-cli-schema").expect("dizin");
    let yol = ornek_veri(&d);
    let metin = basarili(&["schema", yol.to_str().unwrap()]);
    assert!(metin.contains("\"bicim\": \"csv\""), "{metin}");
    assert!(metin.contains("\"ad\""), "{metin}");
    assert!(metin.contains("\"tam_sayi\""), "{metin}");
    assert!(metin.contains("\"satir_sayisi\": 5"), "{metin}");
    assert!(metin.contains("\"bos_oran\""));
}

#[test]
fn schema_cikti_dosyasina_yazilabilir() {
    let d = GeciciDizin::yeni("dl-cli-schema-cikti").expect("dizin");
    let yol = ornek_veri(&d);
    let hedef = d.yol_birestir("sema.json");
    basarili(&[
        "schema",
        yol.to_str().unwrap(),
        "--cikti",
        hedef.to_str().unwrap(),
    ]);
    let metin = std::fs::read_to_string(&hedef).expect("oku");
    assert!(metin.contains("\"sutunlar\""));
}

#[test]
fn count_alt_komutu_ozet_yazar() {
    let d = GeciciDizin::yeni("dl-cli-count").expect("dizin");
    let yol = ornek_veri(&d);
    let metin = basarili(&["count", yol.to_str().unwrap()]);
    assert!(metin.contains("\"satir_sayisi\": 5"), "{metin}");
    assert!(metin.contains("\"sutun_sayisi\": 3"), "{metin}");
    assert!(metin.contains("\"indeks_kipi\": \"seyrek\""), "{metin}");
}

#[test]
fn head_alt_komutu_tablo_baser() {
    let d = GeciciDizin::yeni("dl-cli-head").expect("dizin");
    let yol = ornek_veri(&d);
    let metin = basarili(&["head", yol.to_str().unwrap(), "--adet", "2"]);
    assert!(metin.contains("ali"));
    assert!(metin.contains("ayse"));
    assert!(!metin.contains("veli"), "adet siniri uygulanmali");
    assert!(metin.contains("2 / 5 satir"), "{metin}");
}

#[test]
fn sample_alt_komutu_pencere_baser() {
    let d = GeciciDizin::yeni("dl-cli-sample").expect("dizin");
    let yol = ornek_veri(&d);
    let metin = basarili(&[
        "sample",
        yol.to_str().unwrap(),
        "--baslangic",
        "3",
        "--adet",
        "2",
    ]);
    assert!(metin.contains("ece"));
    assert!(metin.contains("can"));
    assert!(!metin.contains("ali"), "pencere basinda olmamali");
    assert!(metin.contains("2 / 5 satir"), "{metin}");
    assert!(metin.contains("pencere 3..5"), "{metin}");
}

#[test]
fn filter_alt_komutu_eslesme_sayisini_yazar() {
    let d = GeciciDizin::yeni("dl-cli-filter").expect("dizin");
    let yol = ornek_veri(&d);
    let metin = basarili(&[
        "filter",
        yol.to_str().unwrap(),
        "--filtre",
        "yas>30",
        "--adet",
        "5",
    ]);
    assert!(metin.contains("ayse"));
    assert!(metin.contains("ece"));
    assert!(!metin.contains("ali"), "filtre disi satir basilmamali");
    assert!(metin.contains("2 / 5 satir eslesti"), "{metin}");
    assert!(metin.contains("1 kosul"), "{metin}");
}

#[test]
fn filter_coklu_kosul_ve_yuzde_gosterir() {
    let d = GeciciDizin::yeni("dl-cli-coklu").expect("dizin");
    let yol = ornek_veri(&d);
    let metin = basarili(&[
        "filter",
        yol.to_str().unwrap(),
        "--filtre",
        "yas>20",
        "--filtre",
        "ad contains i",
        "--adet",
        "5",
    ]);
    assert!(metin.contains("2 kosul"), "{metin}");
    assert!(metin.contains("%"), "yuzde gosterilmeli: {metin}");
}

#[test]
fn export_alt_komutu_csv_yazar() {
    let d = GeciciDizin::yeni("dl-cli-export").expect("dizin");
    let yol = ornek_veri(&d);
    let hedef = d.yol_birestir("cikti.csv");
    let metin = basarili(&[
        "export",
        yol.to_str().unwrap(),
        "--filtre",
        "yas>30",
        "--cikti",
        hedef.to_str().unwrap(),
    ]);
    assert!(metin.contains("\"yazilan_satir\": 2"), "{metin}");
    let icerik = std::fs::read_to_string(&hedef).expect("oku");
    assert!(icerik.starts_with("no,ad,yas"));
    assert!(icerik.contains("ayse"));
    assert!(!icerik.contains("veli"));
}

#[test]
fn export_alt_komutu_jsonl_yazar() {
    let d = GeciciDizin::yeni("dl-cli-export-jsonl").expect("dizin");
    let yol = d.yol_birestir("veri.jsonl");
    yaz(&yol, b"{\"a\":1,\"b\":true}\n{\"a\":2,\"b\":false}\n");
    let hedef = d.yol_birestir("cikti.jsonl");
    basarili(&[
        "export",
        yol.to_str().unwrap(),
        "--cikti",
        hedef.to_str().unwrap(),
    ]);
    let icerik = std::fs::read_to_string(&hedef).expect("oku");
    assert!(icerik.contains("\"a\":1"));
    assert!(icerik.contains("\"b\":true"));
}

#[test]
fn sutun_secimi_ile_cikti_kisaltilir() {
    let d = GeciciDizin::yeni("dl-cli-sutun").expect("dizin");
    let yol = ornek_veri(&d);
    let metin = basarili(&[
        "head",
        yol.to_str().unwrap(),
        "--sutun",
        "ad",
        "--adet",
        "2",
    ]);
    assert!(metin.contains("ali"));
    assert!(!metin.contains("yas"), "secilmeyen sutun basilmamali");
}

#[test]
fn yogun_indeks_ile_calisir() {
    let d = GeciciDizin::yeni("dl-cli-yogun").expect("dizin");
    let yol = ornek_veri(&d);
    let metin = basarili(&["count", yol.to_str().unwrap(), "--indeks", "yogun"]);
    assert!(metin.contains("\"indeks_kipi\": \"yogun\""), "{metin}");
}

#[test]
fn ozel_ayrac_ile_calisir() {
    let d = GeciciDizin::yeni("dl-cli-ayrac").expect("dizin");
    let yol = d.yol_birestir("veri.dat");
    yaz(&yol, b"a;b\n1;2\n");
    let metin = basarili(&[
        "head",
        yol.to_str().unwrap(),
        "--bicim",
        "virgullu-ayracli",
        "--ayrac",
        ";",
    ]);
    assert!(metin.contains("a"));
    assert!(metin.contains("b"));
}

#[test]
fn olmayan_dosya_hata_mesaji_ve_kod_verir() {
    let cikti = calistir(&["count", "bulunmayan-dosya.csv"]);
    assert!(!cikti.status.success());
    let hata = String::from_utf8_lossy(&cikti.stderr);
    assert!(hata.contains("bulunmayan-dosya.csv"), "{hata}");
    assert!(hata.starts_with("datalens:"), "{hata}");
}

#[test]
fn bilinmeyen_sutun_hata_verir() {
    let d = GeciciDizin::yeni("dl-cli-hatali-sutun").expect("dizin");
    let yol = ornek_veri(&d);
    let cikti = calistir(&["filter", yol.to_str().unwrap(), "--filtre", "olmayan=1"]);
    assert!(!cikti.status.success());
    let hata = String::from_utf8_lossy(&cikti.stderr);
    assert!(hata.contains("sutun bulunamadi"), "{hata}");
}

#[test]
fn bozuk_filtre_ifadesi_hata_verir() {
    let d = GeciciDizin::yeni("dl-cli-bozuk-filtre").expect("dizin");
    let yol = ornek_veri(&d);
    let cikti = calistir(&["filter", yol.to_str().unwrap(), "--filtre", "yas"]);
    assert!(!cikti.status.success());
    let hata = String::from_utf8_lossy(&cikti.stderr);
    assert!(hata.contains("filtre"), "{hata}");
}

#[test]
fn gecersiz_aralik_hata_verir() {
    let d = GeciciDizin::yeni("dl-cli-aralik").expect("dizin");
    let yol = ornek_veri(&d);
    let cikti = calistir(&["sample", yol.to_str().unwrap(), "--baslangic", "99"]);
    assert!(!cikti.status.success());
    let hata = String::from_utf8_lossy(&cikti.stderr);
    assert!(hata.contains("gecersiz satir araligi"), "{hata}");
}

#[test]
fn bilinmeyen_alt_komut_kullanim_hatasi_verir() {
    let cikti = calistir(&["olmayan-komut"]);
    assert!(!cikti.status.success());
}
