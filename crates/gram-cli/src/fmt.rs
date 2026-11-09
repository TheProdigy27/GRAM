//! Mise en forme française des nombres (virgule décimale, espace des milliers).

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
    format!("{} Go", num(bytes as f64 / GIB, 1))
}

/// Octets → « 1 862 Go », sans décimale (disques).
pub fn gb_round(bytes: u64) -> String {
    format!("{} Go", thousands((bytes as f64 / GIB).round() as u64))
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
        assert_eq!(gb_round(1_999_277_912_064), "1 862 Go");
    }
}
