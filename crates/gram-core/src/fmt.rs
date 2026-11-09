//! Mise en forme française des nombres (virgule décimale, espace des milliers), partagée par
//! les explications du classement, l'export et `gram-cli`.

const GIB: f64 = (1u64 << 30) as f64;

/// Nombre à `decimals` décimales, avec une virgule.
pub fn num(x: f64, decimals: usize) -> String {
    format!("{x:.decimals$}").replace('.', ",")
}

/// Entier avec une espace entre les milliers.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Octets → « 15,9 Go » (Gio, comme le Gestionnaire des tâches).
pub fn gb(bytes: u64) -> String {
    format!("{} Go", gb_value(bytes))
}

/// Octets → « 15,9 » (Gio, sans unité).
pub fn gb_value(bytes: u64) -> String {
    num(bytes as f64 / GIB, 1)
}

/// Octets → « 1 862 Go », sans décimale (disques).
pub fn gb_round(bytes: u64) -> String {
    format!("{} Go", thousands((bytes as f64 / GIB).round() as u64))
}

/// Contexte en tokens → « 32k » (multiples de 1 024), « 512 » en dessous.
pub fn ctx_k(tokens: u32) -> String {
    if tokens >= 1024 {
        format!("{}k", tokens / 1024)
    } else {
        tokens.to_string()
    }
}

/// Vitesse → « ~38 », « ~5,6 » sous 10 tok/s, « — » si nulle.
pub fn tok_s(t: f32) -> String {
    match t {
        t if t.is_nan() || t <= 0.0 => "—".into(),
        t if t < 10.0 => format!("~{}", num(f64::from(t), 1)),
        t => format!("~{}", t.round()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(num(96.0, 1), "96,0");
        assert_eq!(thousands(1_862), "1 862");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000_000), "1 000 000");
        assert_eq!(gb(16 << 30), "16,0 Go");
        assert_eq!(gb_value(6 << 29), "3,0");
        assert_eq!(gb_round(1_999_277_912_064), "1 862 Go");
        assert_eq!(ctx_k(32_768), "32k");
        assert_eq!(ctx_k(40_960), "40k");
        assert_eq!(ctx_k(512), "512");
        assert_eq!(tok_s(37.6), "~38");
        assert_eq!(tok_s(5.62), "~5,6");
        assert_eq!(tok_s(0.0), "—");
    }
}
