//! Lien profond `gram-harness://open?model=<nom Ollama encodé en pourcentage>`.
//!
//! N'importe quelle page web peut ouvrir ce lien : l'analyse est stricte, et le lien ne fait
//! que sélectionner un modèle déjà installé dans Ollama (jamais de téléchargement, de
//! suppression ni de commande).

use url::Url;

/// Schéma enregistré par GRAM Harness.
pub const SCHEME: &str = "gram-harness";
/// Longueur maximale d'un nom de modèle, en caractères.
pub const MAX_MODEL_LEN: usize = 200;

/// « Ouvrir ce modèle dans GRAM Harness ».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLink {
    /// Nom du modèle dans Ollama : `gram/qwen3-8b:q4_k_m`.
    pub model: String,
}

/// Pourquoi un lien est refusé (pour le journal : un lien refusé est ignoré).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeepLinkError {
    #[error("adresse invalide : {0}")]
    Url(String),
    #[error("schéma « {0} » au lieu de « {SCHEME} »")]
    Scheme(String),
    #[error("action inconnue : seule « open » existe")]
    Action,
    #[error("partie inattendue dans le lien (identifiant, port ou ancre)")]
    Extra,
    #[error("paramètre inconnu : {0:?}")]
    Param(String),
    #[error("paramètre « model » absent ou vide")]
    MissingModel,
    #[error("paramètre « model » en double")]
    DuplicateModel,
    #[error("nom de modèle trop long ({0} caractères, {MAX_MODEL_LEN} au plus)")]
    TooLong(usize),
    #[error("nom de modèle invalide : {0:?}")]
    InvalidName(String),
}

impl DeepLink {
    /// Lien vers un modèle ; refusé si son nom ne passerait pas [`DeepLink::parse`].
    pub fn new(model: impl Into<String>) -> Result<Self, DeepLinkError> {
        let model = model.into();
        check_name(&model)?;
        Ok(Self { model })
    }

    /// `gram-harness://open?model=gram%2Fqwen3-8b%3Aq4_k_m`
    pub fn to_url(&self) -> String {
        let mut url = Url::parse(&format!("{SCHEME}://open")).expect("adresse fixe");
        url.query_pairs_mut().append_pair("model", &self.model);
        url.into()
    }

    /// Analyse stricte. Refuse un autre schéma, une autre action que `open` (`open/` toléré :
    /// Windows ajoute parfois la barre), un identifiant, un port ou une ancre, un paramètre
    /// autre que `model`, `model` absent, vide ou en double, et un nom de plus de 200
    /// caractères ou qui n'est pas fait de segments `[A-Za-z0-9._-]` séparés par `/`, avec une
    /// étiquette `:` finale facultative (un segment fait seulement de points, comme `..`, est
    /// refusé).
    pub fn parse(link: &str) -> Result<Self, DeepLinkError> {
        let url = Url::parse(link.trim()).map_err(|e| DeepLinkError::Url(e.to_string()))?;
        if url.scheme() != SCHEME {
            return Err(DeepLinkError::Scheme(url.scheme().to_string()));
        }
        if url.host_str() != Some("open") || !matches!(url.path(), "" | "/") {
            return Err(DeepLinkError::Action);
        }
        if !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.fragment().is_some()
        {
            return Err(DeepLinkError::Extra);
        }
        let mut model = None;
        for (key, value) in url.query_pairs() {
            if key != "model" {
                return Err(DeepLinkError::Param(key.into_owned()));
            }
            if model.replace(value.into_owned()).is_some() {
                return Err(DeepLinkError::DuplicateModel);
            }
        }
        let model = model
            .filter(|m| !m.is_empty())
            .ok_or(DeepLinkError::MissingModel)?;
        Self::new(model)
    }
}

fn check_name(name: &str) -> Result<(), DeepLinkError> {
    let len = name.chars().count();
    if len > MAX_MODEL_LEN {
        return Err(DeepLinkError::TooLong(len));
    }
    let part = |s: &str| {
        s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            && s.bytes().any(|b| b != b'.')
    };
    let (path, tag) = match name.rsplit_once(':') {
        Some((path, tag)) => (path, Some(tag)),
        None => (name, None),
    };
    if path.split('/').all(part) && tag.is_none_or(part) {
        Ok(())
    } else {
        Err(DeepLinkError::InvalidName(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(link: &str) -> Result<String, DeepLinkError> {
        DeepLink::parse(link).map(|l| l.model)
    }

    #[test]
    fn aller_retour() {
        let link = DeepLink::new("gram/qwen3-8b:q4_k_m").unwrap();
        assert_eq!(
            link.to_url(),
            "gram-harness://open?model=gram%2Fqwen3-8b%3Aq4_k_m"
        );
        assert_eq!(DeepLink::parse(&link.to_url()), Ok(link));
        for name in [
            "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M",
            "qwen3",
            "gram/x_y.z-1:ud-q4_k_xl",
        ] {
            let link = DeepLink::new(name).unwrap();
            assert_eq!(model(&link.to_url()).unwrap(), name);
        }
    }

    #[test]
    fn formes_acceptees() {
        let ok = Ok("gram/qwen3-8b:q4_k_m".to_string());
        // Sans encodage, avec la barre que Windows ajoute parfois, schéma en majuscules,
        // espaces autour.
        assert_eq!(model("gram-harness://open?model=gram/qwen3-8b:q4_k_m"), ok);
        assert_eq!(
            model("gram-harness://open/?model=gram%2Fqwen3-8b%3Aq4_k_m"),
            ok
        );
        assert_eq!(
            model(" GRAM-HARNESS://open?model=gram%2fqwen3-8b%3aq4_k_m\n"),
            ok
        );
        assert_eq!(model("gram-harness://open?model=qwen3"), Ok("qwen3".into()));
    }

    #[test]
    fn refus() {
        use DeepLinkError::*;
        assert!(matches!(model("pas un lien"), Err(Url(_))));
        // Autre schéma.
        assert_eq!(
            model("https://open?model=qwen3"),
            Err(Scheme("https".into()))
        );
        assert_eq!(model("gram://open?model=qwen3"), Err(Scheme("gram".into())));
        // Autre action.
        assert_eq!(model("gram-harness://delete?model=qwen3"), Err(Action));
        assert_eq!(model("gram-harness://open/chat?model=qwen3"), Err(Action));
        assert_eq!(model("gram-harness://OPEN?model=qwen3"), Err(Action));
        assert_eq!(model("gram-harness:open?model=qwen3"), Err(Action));
        assert_eq!(model("gram-harness:///open?model=qwen3"), Err(Action));
        // Identifiant, port, ancre.
        assert_eq!(model("gram-harness://moi@open?model=qwen3"), Err(Extra));
        assert_eq!(model("gram-harness://open:80?model=qwen3"), Err(Extra));
        assert_eq!(model("gram-harness://open?model=qwen3#x"), Err(Extra));
        // Autre paramètre, `model` absent, vide ou en double.
        assert_eq!(
            model("gram-harness://open?model=qwen3&pull=1"),
            Err(Param("pull".into()))
        );
        assert_eq!(
            model("gram-harness://open?Model=qwen3"),
            Err(Param("Model".into()))
        );
        assert_eq!(model("gram-harness://open"), Err(MissingModel));
        assert_eq!(model("gram-harness://open?model="), Err(MissingModel));
        assert_eq!(
            model("gram-harness://open?model=qwen3&model=llama3"),
            Err(DuplicateModel)
        );
        // Nom trop long.
        let long = format!("gram/{}", "a".repeat(196));
        assert_eq!(long.len(), 201);
        assert_eq!(
            model(&format!("gram-harness://open?model={long}")),
            Err(TooLong(201))
        );
        assert!(model(&format!("gram-harness://open?model={}", &long[1..])).is_ok());
        // Caractères ou forme invalides.
        for bad in [
            "gram%2F..%2F..%5Cx",
            "..%2F..%2Fx",
            "gram%2F.%2Fx",
            "qwen3:..",
            "...",
            "a%20b",
            "a+b",
            "a%00",
            "a/%3Cscript%3E",
            "%C3%A9t%C3%A9",
            "/qwen3",
            "gram//x",
            "gram/",
            "qwen3:",
            ":latest",
            "a:b:c",
            "a:b/c",
        ] {
            assert!(
                matches!(
                    model(&format!("gram-harness://open?model={bad}")),
                    Err(InvalidName(_))
                ),
                "{bad}"
            );
        }
        assert!(DeepLink::new("a b").is_err());
    }
}
