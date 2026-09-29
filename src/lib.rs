//! VeriMercek (DataLens) çekirdek kütüphanesi.
//!
//! Amaç: CSV, TSV ve JSONL dosyalarını **sabit bellekle** açmak, profillemek,
//! filtrelemek ve dışa aktarmak. Dosyanın tamamı hiçbir zaman belleğe
//! alınmaz; yalnızca satır konumları (indeks) ve istenen satır penceresi tutulur.
//!
//! Katmanlar tek yönlü bağımlılık gösterir (rapor b06):
//!
//! ```text
//! hata  <-  bicim  <-  ayristirici  <-  indeks  <-  kaynak
//!                                                  |        |        |
//!                                       profil   filtre  gorunum  disa
//! ```
//!
//! Arayüz katmanı (`gorunum`, `main`) çekirdeğin yalnızca komutlarını çağırır;
//! dosya biçimi ayrıntılarını bilmez.
//!
//! # Güvenlik
//!
//! Tüm kaynak `#![forbid(unsafe_code)]` ile derlenir. Ağ, grafik arayüz ve
//! harici C kütüphanesi kullanılmaz (bkz. `WORKER_CONTRACT.md` § 3.2).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub mod ayristirici;
pub mod bicim;
pub mod disa;
pub mod filtre;
pub mod gorunum;
pub mod hata;
pub mod indeks;
pub mod kaynak;
pub mod profil;
pub mod tip;

#[cfg(test)]
mod test_yardimcisi;
