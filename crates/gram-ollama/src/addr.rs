//! Adresse de l'API locale d'Ollama, d'après la variable `OLLAMA_HOST` (même syntaxe qu'Ollama).

use std::net::{SocketAddr, ToSocketAddrs};

pub const OLLAMA_DEFAULT_PORT: u16 = 11434;

/// Adresse de l'API d'après `OLLAMA_HOST` : `hôte:port`, `http://hôte:port`, `:port`…
/// `0.0.0.0` et `localhost` deviennent `127.0.0.1`. HTTPS n'est pas pris en charge (`None`) :
/// GRAM ne s'adresse qu'à l'Ollama de la machine.
pub fn ollama_addr(host: Option<&str>) -> Option<SocketAddr> {
    let raw = host.map(str::trim).filter(|h| !h.is_empty()).unwrap_or("");
    let (scheme, rest) = raw.split_once("://").unwrap_or(("http", raw));
    if !scheme.eq_ignore_ascii_case("http") {
        return None;
    }
    let authority = rest.split('/').next().unwrap_or("");
    let (h, port) = split_host_port(authority)?;
    let port = port.unwrap_or(if raw.contains("://") && !authority.contains(':') {
        80
    } else {
        OLLAMA_DEFAULT_PORT
    });
    let h = match h {
        "" | "0.0.0.0" | "localhost" => "127.0.0.1",
        "::" => "::1",
        other => other,
    };
    (h, port).to_socket_addrs().ok()?.next()
}

/// Adresse de l'API d'après la variable d'environnement `OLLAMA_HOST` du processus.
pub fn ollama_addr_from_env() -> Option<SocketAddr> {
    ollama_addr(std::env::var("OLLAMA_HOST").ok().as_deref())
}

/// `hôte:port`, `[ipv6]:port`, `hôte`, `:port`.
fn split_host_port(authority: &str) -> Option<(&str, Option<u16>)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (h, after) = rest.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p.parse().ok()?),
            None => None,
        };
        return Some((h, port));
    }
    match authority.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => Some((h, Some(p.parse().ok()?))),
        Some(_) => Some((authority, None)), // IPv6 sans crochets ni port
        None => Some((authority, None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    fn local(port: u16) -> Option<SocketAddr> {
        Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
    }

    #[test]
    fn adresse_ollama() {
        let addr = ollama_addr;
        assert_eq!(addr(None), local(11434));
        assert_eq!(addr(Some("")), local(11434));
        assert_eq!(addr(Some("0.0.0.0")), local(11434));
        assert_eq!(addr(Some("0.0.0.0:8080")), local(8080));
        assert_eq!(addr(Some(":11435")), local(11435));
        assert_eq!(addr(Some("127.0.0.1:11434")), local(11434));
        assert_eq!(addr(Some("http://localhost:11434/")), local(11434));
        assert_eq!(addr(Some("http://127.0.0.1")), local(80));
        assert_eq!(
            addr(Some("[::1]:11434")),
            Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 11434))
        );
        assert_eq!(addr(Some("https://ollama.example")), None);
        assert_eq!(addr(Some("127.0.0.1:pasunport")), None);
    }
}
