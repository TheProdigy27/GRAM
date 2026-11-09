//! Catalogue de démarrage embarqué (§7.7 du plan) : `data/seed_catalog.json`, généré par
//! `gram-cli snapshot`. Au premier lancement hors ligne, l'application fonctionne avec lui.

use crate::catalog::{CATALOG_FORMAT, Catalog};
use std::sync::OnceLock;

const SEED: &[u8] = include_bytes!("../../../data/seed_catalog.json");

/// Le catalogue embarqué, `None` s'il est absent ou d'un autre format.
pub fn seed_catalog() -> Option<&'static Catalog> {
    static SEED_CATALOG: OnceLock<Option<Catalog>> = OnceLock::new();
    SEED_CATALOG
        .get_or_init(|| {
            serde_json::from_slice::<Option<Catalog>>(SEED)
                .inspect_err(|e| tracing::warn!("catalogue embarqué illisible : {e}"))
                .ok()
                .flatten()
                .filter(|c| c.format == CATALOG_FORMAT)
        })
        .as_ref()
}

/// Sérialise un catalogue pour `data/seed_catalog.json` (compact : il est embarqué).
pub fn to_seed_json(c: &Catalog) -> serde_json::Result<Vec<u8>> {
    let mut out = serde_json::to_vec(c)?;
    out.push(b'\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_embarque_valide() {
        let c = seed_catalog().expect("data/seed_catalog.json absent ou invalide");
        // Généré sans borne matérielle : il doit servir sur n'importe quelle machine.
        assert_eq!(c.max_params, None);
        assert!(c.models.len() > 300, "{}", c.models.len());
        let detailed = c.models.iter().filter(|m| !m.variants.is_empty()).count();
        assert!(detailed >= 100, "{detailed}");
        assert!(c.models.iter().any(|m| m.arch.is_some()));
        let json = to_seed_json(c).unwrap();
        assert_eq!(serde_json::from_slice::<Catalog>(&json).unwrap(), *c);
    }
}
