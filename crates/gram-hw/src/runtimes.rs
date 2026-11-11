//! Runtimes installés (§6.4 du plan) : Ollama, LM Studio, llama.cpp.
//!
//! Installé = exécutable trouvé dans le `PATH` ou à l'emplacement d'installation par défaut
//! (dossier des programmes sous Windows, applications `.app` et Homebrew sous macOS).
//! En cours d'exécution = processus présent, ou API locale d'Ollama qui répond.

use gram_core::{Os, RuntimeStatus, RuntimesStatus};
use gram_ollama::ollama_addr;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
const IO_TIMEOUT: Duration = Duration::from_millis(800);

/// Détecte les runtimes. Bloquant (≈ 100 ms ; jusqu'à 1 s si Ollama ne répond pas).
pub fn detect_runtimes() -> RuntimesStatus {
    let env = Env::from_process();
    let ollama_version = ollama_addr(env.ollama_host.as_deref()).and_then(ollama_version);
    let processes = running_processes(&[
        exe("ollama"),
        exe("ollama app"),
        exe("lm studio"),
        exe("lms"),
        exe("llama-server"),
    ]);
    detect_with(&env, &processes, ollama_version)
}

/// Variables d'environnement et dossiers utiles, injectables dans les tests.
#[derive(Debug, Default)]
struct Env {
    os: Os,
    path: Vec<PathBuf>,
    local_app_data: Option<PathBuf>,
    user_profile: Option<PathBuf>,
    program_files: Option<PathBuf>,
    /// Dossier personnel (`HOME`, macOS et Linux).
    home: Option<PathBuf>,
    /// Dossiers des applications macOS : `/Applications`, puis `~/Applications`.
    applications: Vec<PathBuf>,
    ollama_host: Option<String>,
}

/// Dossiers où Homebrew, MacPorts et les installateurs rangent les commandes. Une application
/// lancée depuis le Finder n'hérite pas du `PATH` du terminal (`/usr/bin:/bin:/usr/sbin:/sbin`).
const MACOS_BIN_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"];

impl Env {
    fn from_process() -> Self {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let os = Os::current();
        let home = var("HOME");
        let mut path: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        let mut applications = Vec::new();
        if os == Os::MacOs {
            for dir in MACOS_BIN_DIRS.map(PathBuf::from) {
                if !path.contains(&dir) {
                    path.push(dir);
                }
            }
            applications.push(PathBuf::from("/Applications"));
            applications.extend(home.as_ref().map(|h| h.join("Applications")));
        }
        Self {
            os,
            path,
            local_app_data: var("LOCALAPPDATA"),
            user_profile: var("USERPROFILE"),
            program_files: var("ProgramFiles"),
            home,
            applications,
            ollama_host: std::env::var("OLLAMA_HOST").ok(),
        }
    }

    /// Exécutable d'une application macOS (`Ollama.app/Contents/Resources/ollama`) dans chacun
    /// des dossiers d'applications.
    fn in_apps(&self, app: &str, rel: &str) -> Vec<Option<PathBuf>> {
        self.applications
            .iter()
            .map(|dir| Some(join_rel(&dir.join(app), rel)))
            .collect()
    }
}

/// Nom de processus en minuscules → exécutable (s'il a pu être lu).
type Processes = HashMap<String, Option<PathBuf>>;

fn detect_with(env: &Env, procs: &Processes, ollama_version: Option<String>) -> RuntimesStatus {
    // Emplacements par défaut des installateurs (à compléter si d'autres apparaissent).
    let under = |base: &Option<PathBuf>, rel: &str| base.as_ref().map(|b| join_rel(b, rel));
    let (ollama_known, lm_studio_known) = match env.os {
        Os::Windows => (
            vec![under(&env.local_app_data, r"Programs\Ollama\ollama.exe")],
            vec![
                under(&env.user_profile, r".lmstudio\bin\lms.exe"),
                under(&env.local_app_data, r"Programs\LM Studio\LM Studio.exe"),
                under(&env.program_files, r"LM Studio\LM Studio.exe"),
            ],
        ),
        // L'application Ollama embarque la commande (et crée `/usr/local/bin/ollama`).
        Os::MacOs => (
            env.in_apps("Ollama.app", "Contents/Resources/ollama"),
            [under(&env.home, ".lmstudio/bin/lms")]
                .into_iter()
                .chain(env.in_apps("LM Studio.app", "Contents/MacOS/LM Studio"))
                .collect(),
        ),
        Os::Linux => (vec![], vec![under(&env.home, ".lmstudio/bin/lms")]),
    };

    let mut ollama = status(
        env,
        procs,
        &[exe("ollama")],
        &ollama_known,
        &[exe("ollama"), exe("ollama app")],
    );
    if ollama_version.is_some() {
        ollama.installed = true;
        ollama.running = true;
    }
    ollama.version = ollama_version;

    let lm_studio = status(
        env,
        procs,
        &[exe("lms")],
        &lm_studio_known,
        &[exe("lm studio"), exe("lms")],
    );

    let llama_cpp = status(
        env,
        procs,
        &[exe("llama-server")],
        &[],
        &[exe("llama-server")],
    );

    RuntimesStatus {
        ollama,
        lm_studio,
        llama_cpp,
        // Rempli par l'application (API locale d'Ollama, `gram-ollama`).
        ollama_models: Vec::new(),
    }
}

fn status(
    env: &Env,
    procs: &Processes,
    in_path: &[String],
    known: &[Option<PathBuf>],
    process_names: &[String],
) -> RuntimeStatus {
    let running_exe = process_names.iter().find_map(|n| procs.get(n));
    let path = in_path
        .iter()
        .find_map(|name| find_in_path(&env.path, name))
        .or_else(|| known.iter().flatten().find(|p| p.is_file()).cloned())
        .or_else(|| running_exe.cloned().flatten());
    RuntimeStatus {
        installed: path.is_some() || running_exe.is_some(),
        running: running_exe.is_some(),
        path: path.map(|p| p.display().to_string()),
        version: None,
    }
}

/// Nom d'exécutable en minuscules pour l'OS courant.
fn exe(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn find_in_path(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(name)).find(|p| p.is_file())
}

/// `base` suivi d'un chemin relatif écrit avec `/` ou `\` : segments joints avec le séparateur
/// du système, pour que les emplacements d'un autre OS (simulé dans les tests) restent valides.
fn join_rel(base: &Path, rel: &str) -> PathBuf {
    rel.split(['/', '\\'])
        .fold(base.to_path_buf(), |p, part| p.join(part))
}

/// Processus en cours dont le nom figure dans `names` ; l'exécutable n'est lu que pour eux.
fn running_processes(names: &[String]) -> Processes {
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let lower = |n: &std::ffi::OsStr| n.to_string_lossy().to_lowercase();
    let pids: Vec<Pid> = sys
        .processes()
        .iter()
        .filter(|(_, p)| names.contains(&lower(p.name())))
        .map(|(pid, _)| *pid)
        .collect();
    if pids.is_empty() {
        return Processes::new();
    }
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        false,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    pids.iter()
        .filter_map(|pid| sys.process(*pid))
        .map(|p| (lower(p.name()), p.exe().map(Path::to_path_buf)))
        .collect()
}

/// `GET /api/version` sur l'API locale d'Ollama.
fn ollama_version(addr: SocketAddr) -> Option<String> {
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok()?;
    // Une seule écriture : la requête part d'un bloc et le serveur la lit entièrement avant de
    // répondre (sinon, sous Windows, sa fermeture provoque un RST qui efface la réponse).
    let request = format!(
        "GET /api/version HTTP/1.1\r\nHost: {addr}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).ok()?;
    let mut buf = Vec::new();
    // Une erreur de lecture après réception de la réponse (délai, reset) n'est pas bloquante.
    let _ = stream.take(64 * 1024).read_to_end(&mut buf);
    let version = parse_version_response(&String::from_utf8_lossy(&buf));
    tracing::debug!(%addr, ?version, "sonde Ollama");
    version
}

fn parse_version_response(response: &str) -> Option<String> {
    let (head, body) = response.split_once("\r\n\r\n")?;
    let status = head.lines().next()?.split_whitespace().nth(1)?;
    if status != "200" {
        return None;
    }
    let json = &body[body.find('{')?..=body.rfind('}')?];
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    value
        .get("version")?
        .as_str()
        .map(str::to_string)
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn reponse_version() {
        let ok = "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\n\
                  Content-Length: 20\r\n\r\n{\"version\":\"0.13.1\"}";
        assert_eq!(parse_version_response(ok), Some("0.13.1".into()));
        let chunked = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                       14\r\n{\"version\":\"0.13.1\"}\r\n0\r\n\r\n";
        assert_eq!(parse_version_response(chunked), Some("0.13.1".into()));
        let not_found = "HTTP/1.1 404 Not Found\r\n\r\n{\"version\":\"x\"}";
        assert_eq!(parse_version_response(not_found), None);
        assert_eq!(
            parse_version_response("HTTP/1.1 200 OK\r\n\r\n<html>"),
            None
        );
        assert_eq!(parse_version_response(""), None);
    }

    #[test]
    fn sonde_sur_un_faux_serveur() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut req = Vec::new();
            let mut chunk = [0u8; 256];
            while !req.ends_with(b"\r\n\r\n") {
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0, "requête incomplète");
                req.extend_from_slice(&chunk[..n]);
            }
            assert!(req.starts_with(b"GET /api/version HTTP/1.1\r\n"));
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\n{\"version\":\"0.13.1\"}")
                .unwrap();
        });
        assert_eq!(ollama_version(addr), Some("0.13.1".into()));
        server.join().unwrap();
    }

    #[test]
    fn sonde_sans_serveur() {
        // Port réservé puis libéré : personne n'écoute.
        let addr = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        assert_eq!(ollama_version(addr), None);
    }

    /// Dossier temporaire propre au test, supprimé à la fin.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("gram-hw-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn touch(&self, rel: &[&str]) -> PathBuf {
            let p = rel.iter().fold(self.0.clone(), |p, part| p.join(part));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"").unwrap();
            p
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rien_d_installe() {
        let tmp = TempDir::new("vide");
        let env = Env {
            path: vec![tmp.0.clone()],
            local_app_data: Some(tmp.0.clone()),
            home: Some(tmp.0.clone()),
            applications: vec![tmp.0.clone()],
            ..Default::default()
        };
        assert_eq!(
            detect_with(&env, &Processes::new(), None),
            RuntimesStatus::default()
        );
    }

    #[test]
    fn trouve_dans_le_path_et_aux_emplacements_par_defaut() {
        let tmp = TempDir::new("installes");
        let bin = tmp.0.join("bin");
        let server = tmp.touch(&["bin", &exe("llama-server")]);
        let ollama = tmp.touch(&["Programs", "Ollama", "ollama.exe"]);
        let env = Env {
            os: Os::Windows,
            path: vec![tmp.0.join("absent"), bin],
            local_app_data: Some(tmp.0.clone()),
            ..Default::default()
        };
        let r = detect_with(&env, &Processes::new(), None);
        assert!(r.llama_cpp.installed && !r.llama_cpp.running);
        assert_eq!(r.llama_cpp.path, Some(server.display().to_string()));
        assert!(r.ollama.installed);
        assert_eq!(r.ollama.path, Some(ollama.display().to_string()));
        assert!(!r.lm_studio.installed);
    }

    #[test]
    fn emplacements_macos() {
        let tmp = TempDir::new("macos");
        let ollama = tmp.touch(&[
            "Applications",
            "Ollama.app",
            "Contents",
            "Resources",
            "ollama",
        ]);
        let lm_studio = tmp.touch(&[
            "Users",
            "moi",
            "Applications",
            "LM Studio.app",
            "Contents",
            "MacOS",
            "LM Studio",
        ]);
        let env = Env {
            os: Os::MacOs,
            path: vec![tmp.0.join("usr-bin")],
            home: Some(tmp.0.join("Users").join("moi")),
            applications: vec![
                tmp.0.join("Applications"),
                tmp.0.join("Users").join("moi").join("Applications"),
            ],
            ..Default::default()
        };
        let r = detect_with(&env, &Processes::new(), None);
        assert!(r.ollama.installed && !r.ollama.running);
        assert_eq!(r.ollama.path, Some(ollama.display().to_string()));
        assert!(r.lm_studio.installed);
        assert_eq!(r.lm_studio.path, Some(lm_studio.display().to_string()));
        assert!(!r.llama_cpp.installed);

        // La commande `lms` passe avant l'application.
        let lms = tmp.touch(&["Users", "moi", ".lmstudio", "bin", "lms"]);
        let r = detect_with(&env, &Processes::new(), None);
        assert_eq!(r.lm_studio.path, Some(lms.display().to_string()));
    }

    #[test]
    fn path_complete_pour_les_applications_du_finder() {
        if cfg!(target_os = "macos") {
            let env = Env::from_process();
            for dir in MACOS_BIN_DIRS {
                assert!(env.path.contains(&PathBuf::from(dir)), "{dir}");
            }
            assert_eq!(env.applications[0], PathBuf::from("/Applications"));
        }
    }

    #[test]
    fn processus_et_api_marquent_en_cours() {
        let env = Env::default();
        let exe_path = PathBuf::from(r"D:\outils\llama-server.exe");
        let procs = Processes::from([
            (exe("llama-server"), Some(exe_path.clone())),
            (exe("lm studio"), None),
        ]);
        let r = detect_with(&env, &procs, Some("0.13.1".into()));
        assert!(r.ollama.installed && r.ollama.running);
        assert_eq!(r.ollama.version.as_deref(), Some("0.13.1"));
        assert!(r.llama_cpp.installed && r.llama_cpp.running);
        assert_eq!(r.llama_cpp.path, Some(exe_path.display().to_string()));
        assert!(r.lm_studio.installed && r.lm_studio.running);
        assert_eq!(r.lm_studio.path, None);
    }
}
