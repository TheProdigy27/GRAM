//! Images jointes à un message (modèles qui ont la vision) : PNG, JPEG ou WebP de 10 Mo au plus,
//! 4 par message. L'interface les envoie en base64 ; le type est reconnu aux premiers octets du
//! fichier, pas au nom ni au type annoncé. Elles sont enregistrées à côté de la conversation
//! (`conversations/<id>/`) et envoyées à Ollama en base64.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// Taille maximale d'une image, en octets (10 Mo, comme les tailles de GRAM : 1 Mo = 2^20).
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
/// Images au plus par message.
pub const MAX_IMAGES: usize = 4;

/// Formats acceptés.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
    Webp,
}

impl ImageKind {
    pub const ALL: [Self; 3] = [Self::Png, Self::Jpeg, Self::Webp];

    /// Reconnu aux premiers octets du fichier.
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            Some(Self::Webp)
        } else {
            None
        }
    }

    /// D'après l'extension d'un nom de fichier enregistré.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.rsplit_once('.')?.1 {
            "png" => Some(Self::Png),
            "jpg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
        }
    }
}

/// Image reçue de l'interface, décodée et vérifiée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub kind: ImageKind,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("Image {0} illisible : la joindre de nouveau")]
    Invalid(usize),
    #[error("Image {0} trop lourde ({1}) : 10 Mo au plus")]
    TooLarge(usize, String),
    #[error("Image {0} dans un format non pris en charge : PNG, JPEG ou WebP")]
    Unsupported(usize),
    #[error("{0} images jointes : 4 au plus par message")]
    TooMany(usize),
}

/// Décode les images d'un message (base64, sans préfixe `data:`). `n`, dans les messages
/// d'erreur, est le rang de l'image (à partir de 1).
pub fn decode_all(uploads: &[String]) -> Result<Vec<Image>, ImageError> {
    if uploads.len() > MAX_IMAGES {
        return Err(ImageError::TooMany(uploads.len()));
    }
    uploads
        .iter()
        .enumerate()
        .map(|(i, data)| decode(i + 1, data))
        .collect()
}

fn decode(n: usize, data: &str) -> Result<Image, ImageError> {
    let data = data.trim();
    // Taille vérifiée avant de décoder : 4 caractères pour 3 octets.
    let approx = data.len() / 4 * 3;
    if approx > MAX_IMAGE_BYTES + 3 {
        return Err(ImageError::TooLarge(n, megabytes(approx)));
    }
    let bytes = STANDARD.decode(data).map_err(|_| ImageError::Invalid(n))?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ImageError::TooLarge(n, megabytes(bytes.len())));
    }
    let kind = ImageKind::detect(&bytes).ok_or(ImageError::Unsupported(n))?;
    Ok(Image { kind, bytes })
}

/// Contenu en base64, comme l'attend Ollama.
pub fn to_base64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Adresse `data:` d'une image enregistrée, que la WebView affiche (la CSP autorise `data:`).
pub fn data_url(kind: ImageKind, bytes: &[u8]) -> String {
    format!("data:{};base64,{}", kind.mime(), to_base64(bytes))
}

/// « 14,2 Mo ».
fn megabytes(bytes: usize) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    format!("{mb:.1} Mo").replace('.', ",")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// PNG de 1 × 1 pixel.
    pub(crate) const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8,
        0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    /// Début d'un JPEG et d'un WebP.
    pub(crate) const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F'];
    pub(crate) const WEBP: &[u8] = b"RIFF\x24\x00\x00\x00WEBPVP8 ";

    #[test]
    fn formats_reconnus_aux_premiers_octets() {
        assert_eq!(ImageKind::detect(PNG), Some(ImageKind::Png));
        assert_eq!(ImageKind::detect(JPEG), Some(ImageKind::Jpeg));
        assert_eq!(ImageKind::detect(WEBP), Some(ImageKind::Webp));
        for other in [
            &b"GIF89a\x01\x00"[..],
            b"RIFF\x24\x00\x00\x00WAVE",
            b"<svg",
            b"",
        ] {
            assert_eq!(ImageKind::detect(other), None);
        }
        for kind in ImageKind::ALL {
            let name = format!("20261117-100000000-1.{}", kind.extension());
            assert_eq!(ImageKind::from_name(&name), Some(kind));
        }
        assert_eq!(ImageKind::from_name("a.gif"), None);
        assert_eq!(ImageKind::from_name("png"), None);
    }

    #[test]
    fn decodage_et_refus() {
        let images = decode_all(&[to_base64(PNG), format!(" {} ", to_base64(JPEG))]).unwrap();
        assert_eq!(
            images[0],
            Image {
                kind: ImageKind::Png,
                bytes: PNG.to_vec()
            }
        );
        assert_eq!(images[1].kind, ImageKind::Jpeg);
        assert!(decode_all(&[]).unwrap().is_empty());

        let err = |uploads: &[String]| decode_all(uploads).unwrap_err().to_string();
        assert_eq!(
            err(&[to_base64(PNG), "pas du base64 !".into()]),
            "Image 2 illisible : la joindre de nouveau"
        );
        assert_eq!(
            err(&[to_base64(b"GIF89a")]),
            "Image 1 dans un format non pris en charge : PNG, JPEG ou WebP"
        );
        let mut big = PNG.to_vec();
        big.resize(MAX_IMAGE_BYTES + 1, 0);
        assert_eq!(
            err(&[to_base64(&big)]),
            "Image 1 trop lourde (10,0 Mo) : 10 Mo au plus"
        );
        let mut huge = PNG.to_vec();
        huge.resize(15 * 1024 * 1024, 0);
        assert_eq!(
            err(&[to_base64(&huge)]),
            "Image 1 trop lourde (15,0 Mo) : 10 Mo au plus"
        );
        // 10 Mo tout juste : accepté.
        big.truncate(MAX_IMAGE_BYTES);
        assert_eq!(
            decode_all(&[to_base64(&big)]).unwrap()[0].bytes.len(),
            MAX_IMAGE_BYTES
        );
        let five = vec![to_base64(PNG); 5];
        assert_eq!(err(&five), "5 images jointes : 4 au plus par message");
    }

    #[test]
    fn adresse_data() {
        assert_eq!(
            data_url(ImageKind::Webp, b"abc"),
            "data:image/webp;base64,YWJj"
        );
    }
}
