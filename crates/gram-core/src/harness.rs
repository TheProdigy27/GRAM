//! Ajout d'un modèle à GRAM Harness, partie pure : progression du téléchargement dans Ollama et
//! place sur le disque. Les requêtes à Ollama sont dans `gram-ollama` (`install`).

use crate::hardware::DiskInfo;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;

/// Étape de l'ajout, dans l'ordre où elles se suivent.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub enum InstallPhase {
    /// Modèles installés, puis manifeste du dépôt.
    #[default]
    Preparing,
    /// Couches du modèle en cours de téléchargement.
    Downloading,
    /// Ollama vérifie les empreintes et écrit le manifeste : aucune nouvelle pendant ce temps
    /// (environ une minute pour 40 Go sur un SSD NVMe).
    Verifying,
    /// Création du modèle dérivé, qui règle le contexte.
    Creating,
}

/// Avancement de l'ajout à un instant donné.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullProgress {
    pub phase: InstallPhase,
    /// Octets sur le disque, y compris ceux d'un téléchargement repris.
    pub completed: u64,
    /// Taille des couches annoncées jusqu'ici : elle grandit quand une couche apparaît.
    pub total: u64,
    /// Débit sur les [`RATE_WINDOW`] dernières secondes, pendant le téléchargement et après
    /// au moins une seconde de suivi.
    pub bytes_per_s: Option<u64>,
}

/// Fenêtre de calcul du débit.
pub const RATE_WINDOW: Duration = Duration::from_secs(3);
/// En dessous, le débit n'est pas encore significatif.
const MIN_RATE_SPAN: Duration = Duration::from_secs(1);

/// Suivi d'un téléchargement d'après les lignes de `POST /api/pull`.
///
/// Ollama envoie, couche par couche, la taille (`total`) et les octets reçus (`completed`). Les
/// totaux s'additionnent sur les couches vues. Le débit ne compte que les octets reçus pendant
/// le suivi : une couche déjà présente ou un téléchargement repris arrive d'un coup, sans être
/// pris pour un débit.
///
/// `now` est le temps écoulé depuis une origine quelconque, la même pour tous les appels.
#[derive(Debug, Clone, Default)]
pub struct PullTracker {
    phase: InstallPhase,
    layers: Vec<Layer>,
    /// Octets reçus depuis le début du suivi.
    received: u64,
    /// `(instant, received)` : un échantillon antérieur à la fenêtre, puis ceux de la fenêtre.
    samples: VecDeque<(Duration, u64)>,
}

#[derive(Debug, Clone)]
struct Layer {
    digest: String,
    total: u64,
    completed: u64,
}

impl PullTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn phase(&self) -> InstallPhase {
        self.phase
    }

    /// Une ligne de progression d'une couche. `completed` manque tant qu'Ollama n'a rien reçu
    /// pour elle : la dernière valeur connue reste.
    pub fn layer(&mut self, digest: &str, total: u64, completed: Option<u64>, now: Duration) {
        match self.layers.iter_mut().find(|l| l.digest == digest) {
            Some(l) => {
                let completed = completed.unwrap_or(l.completed);
                self.received += completed.saturating_sub(l.completed);
                l.total = total;
                l.completed = completed;
            }
            None => self.layers.push(Layer {
                digest: digest.to_string(),
                total,
                completed: completed.unwrap_or(0),
            }),
        }
        self.samples.push_back((now, self.received));
        if let Some(start) = now.checked_sub(RATE_WINDOW) {
            while self.samples.len() > 1 && self.samples[1].0 <= start {
                self.samples.pop_front();
            }
        }
        self.set_phase(InstallPhase::Downloading);
    }

    /// Passe à l'étape `phase` ; une étape déjà dépassée est ignorée.
    pub fn set_phase(&mut self, phase: InstallPhase) {
        self.phase = self.phase.max(phase);
    }

    pub fn progress(&self, now: Duration) -> PullProgress {
        let total = self.layers.iter().map(|l| l.total).sum();
        // La dernière ligne d'une couche n'atteint pas toujours `total` : une fois la
        // vérification commencée, tout est arrivé.
        let completed = if self.phase >= InstallPhase::Verifying {
            total
        } else {
            self.layers.iter().map(|l| l.completed.min(l.total)).sum()
        };
        PullProgress {
            phase: self.phase,
            completed,
            total,
            bytes_per_s: if self.phase == InstallPhase::Downloading {
                self.rate(now)
            } else {
                None
            },
        }
    }

    /// Octets reçus depuis le dernier échantillon antérieur à la fenêtre (à défaut, le
    /// premier), divisés par le temps écoulé jusqu'à `now` : le débit retombe à zéro quand
    /// plus rien n'arrive.
    fn rate(&self, now: Duration) -> Option<u64> {
        let &(_, last) = self.samples.back()?;
        let window_start = now.checked_sub(RATE_WINDOW);
        let &(t0, r0) = window_start
            .and_then(|s| self.samples.iter().rev().find(|(t, _)| *t <= s))
            .or(self.samples.front())?;
        let span = now.checked_sub(t0)?;
        (span >= MIN_RATE_SPAN).then(|| ((last - r0) as f64 / span.as_secs_f64()).round() as u64)
    }
}

/// Place sur le disque qui recevra un téléchargement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskSpace {
    /// Point de montage du disque (« C:\ », « / »…).
    pub mount: String,
    /// Espace libre, en octets.
    pub available: u64,
    /// Place demandée : la taille plus 10 %.
    pub needed: u64,
    pub enough: bool,
}

/// Le disque qui contient `dir` est celui dont le point de montage est le plus long préfixe du
/// dossier (montages imbriqués) ; assez de place si l'espace libre couvre `size` plus 10 %.
/// `None` si aucun disque connu ne contient le dossier.
pub fn disk_check(disks: &[DiskInfo], dir: &str, size: u64) -> Option<DiskSpace> {
    let (_, disk) = disks
        .iter()
        .filter_map(|d| mount_prefix_len(&d.mount, dir).map(|len| (len, d)))
        .max_by_key(|&(len, _)| len)?;
    let needed = size.saturating_add(size / 10);
    Some(DiskSpace {
        mount: disk.mount.clone(),
        available: disk.available,
        needed,
        enough: disk.available >= needed,
    })
}

/// Longueur du point de montage s'il contient `dir`, segment par segment (`/mnt/d` ne contient
/// pas `/mnt/data`). Les chemins Windows se comparent sans tenir compte de la casse, avec `\`
/// ou `/`.
fn mount_prefix_len(mount: &str, dir: &str) -> Option<usize> {
    let windows = is_windows_path(mount) || is_windows_path(dir);
    let normalize = |s: &str| {
        if windows {
            s.replace('\\', "/").to_ascii_lowercase()
        } else {
            s.to_string()
        }
    };
    let (mount, dir) = (normalize(mount), normalize(dir));
    let mount = mount.trim_end_matches('/');
    let rest = dir.strip_prefix(mount)?;
    (rest.is_empty() || rest.starts_with('/')).then_some(mount.len())
}

/// `C:\…`, `d:/…`
fn is_windows_path(path: &str) -> bool {
    matches!(path.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn couches_additionnees_et_reprise() {
        let mut t = PullTracker::new();
        assert_eq!(t.progress(ms(0)), PullProgress::default());

        // Couche reprise : 40 Mo déjà là, comptés dans l'avancement mais pas dans le débit.
        t.layer("sha256:a", 175_000_000, Some(40_000_000), ms(0));
        // Deuxième couche annoncée sans `completed`.
        t.layer("sha256:b", 100_000_000, None, ms(100));
        let p = t.progress(ms(100));
        assert_eq!(p.phase, InstallPhase::Downloading);
        assert_eq!((p.completed, p.total), (40_000_000, 275_000_000));
        assert_eq!(p.bytes_per_s, None, "moins d'une seconde de suivi");

        t.layer("sha256:a", 175_000_000, Some(60_000_000), ms(1000));
        t.layer("sha256:b", 100_000_000, None, ms(1500));
        t.layer("sha256:b", 100_000_000, Some(10_000_000), ms(2000));
        let p = t.progress(ms(2000));
        assert_eq!(p.completed, 70_000_000);
        // 30 Mo reçus en 2 s.
        assert_eq!(p.bytes_per_s, Some(15_000_000));

        // `completed` au-delà de `total` n'est pas compté deux fois.
        t.layer("sha256:c", 500, Some(600), ms(2100));
        assert_eq!(t.progress(ms(2100)).completed, 70_000_500);
    }

    #[test]
    fn debit_sur_trois_secondes() {
        let mut t = PullTracker::new();
        // 10 Mo/s pendant 5 s, puis 1 Mo/s.
        for s in 0..=5 {
            t.layer("sha256:a", u64::MAX / 2, Some(s * 10_000_000), ms(s * 1000));
        }
        assert_eq!(t.progress(ms(5000)).bytes_per_s, Some(10_000_000));
        for s in 6..=8 {
            let completed = 50_000_000 + (s - 5) * 1_000_000;
            t.layer("sha256:a", u64::MAX / 2, Some(completed), ms(s * 1000));
        }
        assert_eq!(t.progress(ms(8000)).bytes_per_s, Some(1_000_000));
        // Plus rien n'arrive : le débit retombe (2 Mo depuis 6 s, en 3,5 s), puis s'annule.
        assert_eq!(t.progress(ms(9500)).bytes_per_s, Some(571_429));
        assert_eq!(t.progress(ms(12_000)).bytes_per_s, Some(0));
        // Seuls les échantillons utiles sont gardés.
        assert!(t.samples.len() <= 4, "{:?}", t.samples);
    }

    #[test]
    fn etapes() {
        let mut t = PullTracker::new();
        assert_eq!(t.phase(), InstallPhase::Preparing);
        t.layer("sha256:a", 1000, Some(990), ms(0));
        t.layer("sha256:a", 1000, Some(999), ms(2000));
        assert_eq!(t.progress(ms(2000)).completed, 999);
        assert!(t.progress(ms(2000)).bytes_per_s.is_some());

        // La dernière ligne n'atteint pas `total` : la vérification vaut 100 %.
        t.set_phase(InstallPhase::Verifying);
        let p = t.progress(ms(2000));
        assert_eq!(
            (p.phase, p.completed, p.total),
            (InstallPhase::Verifying, 1000, 1000)
        );
        assert_eq!(p.bytes_per_s, None);

        // Une étape ne recule pas.
        t.layer("sha256:a", 1000, Some(1000), ms(3000));
        t.set_phase(InstallPhase::Preparing);
        assert_eq!(t.phase(), InstallPhase::Verifying);
        t.set_phase(InstallPhase::Creating);
        assert_eq!(t.progress(ms(3000)).phase, InstallPhase::Creating);
    }

    #[test]
    fn progression_en_camel_case() {
        let p = PullProgress {
            phase: InstallPhase::Downloading,
            completed: 1,
            total: 2,
            bytes_per_s: Some(3),
        };
        assert_eq!(
            serde_json::to_value(p).unwrap(),
            serde_json::json!({ "phase": "downloading", "completed": 1, "total": 2, "bytesPerS": 3 })
        );
    }

    fn disk(mount: &str, available: u64) -> DiskInfo {
        DiskInfo {
            mount: mount.into(),
            total: available * 2,
            available,
        }
    }

    #[test]
    fn disque_du_dossier() {
        let disks = [
            disk("/", 100),
            disk("/home", 200),
            disk("/mnt/d", 300),
            disk("/home/gram/data", 400),
        ];
        let mount = |dir| disk_check(&disks, dir, 0).map(|d| d.mount);
        // Montages imbriqués : le plus long préfixe.
        assert_eq!(mount("/home/gram/.ollama/models").as_deref(), Some("/home"));
        assert_eq!(
            mount("/home/gram/data/ollama").as_deref(),
            Some("/home/gram/data")
        );
        assert_eq!(mount("/home/gram/data").as_deref(), Some("/home/gram/data"));
        // Segment par segment : `/mnt/d` ne contient pas `/mnt/data`.
        assert_eq!(mount("/mnt/data/models").as_deref(), Some("/"));
        assert_eq!(mount("/mnt/d/models").as_deref(), Some("/mnt/d"));
        assert_eq!(mount("relatif/models"), None);

        let windows = [
            disk("C:\\", 100),
            disk("D:\\", 200),
            disk("C:\\mnt\\rapide\\", 300),
        ];
        let mount = |dir| disk_check(&windows, dir, 0).map(|d| d.mount);
        assert_eq!(
            mount("C:\\Users\\gram\\.ollama\\models").as_deref(),
            Some("C:\\")
        );
        // `OLLAMA_MODELS` sur un autre disque, casse et séparateurs libres.
        assert_eq!(mount("d:/Ollama/models").as_deref(), Some("D:\\"));
        assert_eq!(
            mount("C:\\MNT\\Rapide\\ollama").as_deref(),
            Some("C:\\mnt\\rapide\\")
        );
        assert_eq!(mount("C:\\mnt\\rapidement").as_deref(), Some("C:\\"));
        assert_eq!(mount("E:\\ollama"), None);
        assert_eq!(disk_check(&[], "C:\\", 1), None);
    }

    #[test]
    fn marge_de_dix_pour_cent() {
        let check = |available, size| disk_check(&[disk("C:\\", available)], "C:\\x", size);
        let ok = check(1100, 1000).unwrap();
        assert_eq!(
            ok,
            DiskSpace {
                mount: "C:\\".into(),
                available: 1100,
                needed: 1100,
                enough: true,
            }
        );
        assert!(!check(1099, 1000).unwrap().enough);
        // Rien à télécharger : toujours assez.
        assert!(check(0, 0).unwrap().enough);
        assert_eq!(check(1, u64::MAX).unwrap().needed, u64::MAX);
    }
}
