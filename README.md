# UDF Okuyucu
**UDF dosyalarını kolayca görüntülemek için geliştirilen açık kaynaklı bir araç.**

> [!WARNING]
> **Bu proje resmî bir UYAP veya Adalet Bakanlığı uygulaması değildir.** Herhangi bir kamu kurumuyla resmî bir bağlantısı bulunmamaktadır.

> [!NOTE]
> Bu projenin geliştirilmesinde yapay zekâ araçlarından yararlanılmıştır.

UDF Okuyucu, UYAP (Ulusal Yargı Ağı Bilişim Sistemi) kapsamında kullanılan `.udf` uzantılı dosyaların vatandaşlar tarafından daha kolay görüntülenebilmesini amaçlayan bir projedir.

### Gereksinimler
Proje, tek başına UDF dosyalarını okuyamaz, UDF dosyalarını okuyabilmek için [udflib](https://github.com/kayrakrh/udflib)'den faydalanır

### Projenin Amacı

UDF dosyalarına erişimi kolaylaştırmak ve vatandaşların bu dosyaların içeriklerini daha rahat inceleyebilmesine yardımcı olmak.

### Katkıda Bulunma

Projeye katkıda bulunmak, hata bildirmek veya geliştirme önerilerinizi paylaşmak isterseniz GitHub üzerinden katkıda bulunabilirsiniz.

## İndirme
### Hazır Paketler
| Paket | Durum |
| --- | --- |
| Appimage | Üzerinde uğraşılıyor |
| Deb | Üzerinde uğraşılıyor | 

### Derleme
Öncelikle [udflib](https://github.com/kayrakrh/udflib)'i sisteminize kurmalısınız. Ardından,
```
cargo run
```
