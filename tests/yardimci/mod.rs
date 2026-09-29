//! Entegrasyon testleri için geçici dosya yardımcısı.
//!
//! Neden `tempfile` yok: bağımlılık politikası (`WORKER_CONTRACT.md` § 3.2)
//! `tempfile`'i hiçbir projede vermez; yardımcı kendi kodumuzla yazılır.
//!
//! `tests/` altındaki her dosya ayrı bir test ikilisi olduğundan bu modül
//! `#[path]` ile paylaşılır.

use std::io;
use std::path::PathBuf;

/// Test içinde geçici dosya/dizin üreten, `Drop` ile temizleyen kapsayıcı.
pub struct GeciciDizin {
    yol: PathBuf,
}

impl GeciciDizin {
    /// `std::env::temp_dir()` altında, etiketten türetilmiş bir dizin oluşturur.
    pub fn yeni(etiket: &str) -> io::Result<Self> {
        let kok = std::env::temp_dir().join(format!("{etiket}-{}", std::process::id()));
        // Aynı etiketle ikinci bir çalıştırma eski içeriği bulabilir; temizle.
        let _ = std::fs::remove_dir_all(&kok);
        std::fs::create_dir_all(&kok)?;
        Ok(Self { yol: kok })
    }

    /// Dizin içine göreli yol oluşturur (dosya yaratmaz).
    pub fn yol_birestir(&self, ad: &str) -> PathBuf {
        self.yol.join(ad)
    }
}

impl Drop for GeciciDizin {
    fn drop(&mut self) {
        // `Drop` içinden hata döndürülemez; temizlik başarısız olsa da testi
        // düşürmemelidir (README'de belgelenmiş istisna).
        let _ = std::fs::remove_dir_all(&self.yol);
    }
}
