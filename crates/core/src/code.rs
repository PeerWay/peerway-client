//! Short, human friendly user codes.
//!
//! Codes use Crockford's base32 alphabet (no I, L, O or U) which avoids the
//! classic "is that a zero or an O?" mistakes when a code is read aloud.

pub const CODE_LEN: usize = 8;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Codes kept for demos and internal tests. Never handed out to accounts.
const RESERVED: &[&str] = &["K7M2QX9A"];

/// Staff/test accounts use this leading character, so it is never generated.
const RESERVED_PREFIX: char = 'Z';

/// Temporary, device-local codes live in their own range so a permanent
/// account can never be assigned one. `T` is not used by staff codes.
const EPHEMERAL_PREFIX: char = 'T';

pub fn is_reserved(code: &str) -> bool {
    code.starts_with(RESERVED_PREFIX) || RESERVED.contains(&code)
}

/// Temporary code that belongs to the reserved ephemeral range.
pub fn is_ephemeral(code: &str) -> bool {
    code.starts_with(EPHEMERAL_PREFIX)
}

fn from_bits(bytes: [u8; 5]) -> String {
    // 5 bytes = 40 bits; 8 groups of 5 bits use the whole space.
    let bits = u64::from_be_bytes([0, 0, 0, bytes[0], bytes[1], bytes[2], bytes[3], bytes[4]]);
    let mut out = String::with_capacity(CODE_LEN);
    for i in 0..CODE_LEN {
        let shift = 40 - 5 * (i + 1);
        let idx = ((bits >> shift) & 0x1f) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

/// Stable LAN pseudonym derived from the device key.
///
/// Broadcast announcements must not expose the permanent code to everyone on
/// a shared network, but peers still need a stable id to recognise a device.
pub fn lan_alias(ed25519_pub_hex: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(ed25519_pub_hex.trim().to_ascii_lowercase().as_bytes());
    let mut bytes = [0u8; 5];
    bytes.copy_from_slice(&digest[..5]);
    from_bits(bytes)
}

/// Generate a fresh permanent code. It never lands in the staff or the
/// ephemeral range.
pub fn generate() -> String {
    for _ in 0..64 {
        let mut bytes = [0u8; 5];
        getrandom::fill(&mut bytes).expect("operating system RNG is available");
        let code = from_bits(bytes);
        if !is_reserved(&code) && !is_ephemeral(&code) {
            return code;
        }
    }
    // One leading character is reserved, so force the first symbol out of it.
    let mut code = generate();
    code.replace_range(0..1, "0");
    code
}

/// Fresh temporary code. It always starts with `T`, a range the server never
/// assigns to accounts, so it cannot collide with a permanent code.
pub fn generate_ephemeral() -> String {
    let mut bytes = [0u8; 5];
    for _ in 0..64 {
        getrandom::fill(&mut bytes).expect("operating system RNG is available");
        let mut code = from_bits(bytes);
        code.replace_range(0..1, "T");
        if !RESERVED.contains(&code.as_str()) {
            return code;
        }
    }
    "T0000000".to_string()
}

/// Normalise user input: uppercase, remove separators, map look-alikes.
pub fn normalize(input: &str) -> Option<String> {
    let mut out = String::with_capacity(CODE_LEN);
    for ch in input.chars() {
        let ch = ch.to_ascii_uppercase();
        match ch {
            ' ' | '-' | '_' | '.' | '\t' | '\n' | '\r' => continue,
            '0' | 'O' => out.push('0'),
            '1' | 'I' | 'L' => out.push('1'),
            c if ALPHABET.contains(&(c as u8)) => out.push(c),
            _ => return None,
        }
    }
    if out.len() == CODE_LEN {
        Some(out)
    } else {
        None
    }
}

/// Human display form, grouped as `ABCD-EFGH`.
pub fn format(code: &str) -> String {
    if code.len() == CODE_LEN {
        format!("{}-{}", &code[..4], &code[4..])
    } else {
        code.to_string()
    }
}

pub fn is_valid(code: &str) -> bool {
    code.len() == CODE_LEN && code.bytes().all(|b| ALPHABET.contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_hands_out_reserved_codes() {
        for _ in 0..500 {
            let code = generate();
            assert!(!code.starts_with('Z'), "{code} uses the reserved prefix");
            assert_ne!(code, "K7M2QX9A");
            assert!(is_valid(&code));
        }
    }

    #[test]
    fn generated_codes_are_valid() {
        for _ in 0..1000 {
            let code = generate();
            assert_eq!(code.len(), CODE_LEN);
            assert!(is_valid(&code), "{code}");
        }
    }

    #[test]
    fn keeps_permanent_and_ephemeral_ranges_apart() {
        for _ in 0..500 {
            let permanent = generate();
            assert!(!is_reserved(&permanent), "{permanent}");
            assert!(!is_ephemeral(&permanent), "{permanent}");
            let ephemeral = generate_ephemeral();
            assert!(is_valid(&ephemeral), "{ephemeral}");
            assert!(is_ephemeral(&ephemeral), "{ephemeral}");
            assert!(!is_reserved(&ephemeral), "{ephemeral}");
        }
    }

    #[test]
    fn normalizes_input() {
        assert_eq!(normalize("abcd-efgh").as_deref(), Some("ABCDEFGH"));
        assert_eq!(normalize(" O1IL-2-345 ").as_deref(), Some("01112345"));
        assert_eq!(normalize("short"), None);
        assert_eq!(normalize("ABCD-EFG!"), None);
    }

    #[test]
    fn formats_with_dash() {
        assert_eq!(format("ABCDEFGH"), "ABCD-EFGH");
    }

    #[test]
    fn lan_alias_is_stable_and_valid() {
        let first = "a".repeat(64);
        let second = "b".repeat(64);
        let alias = lan_alias(&first);
        assert!(is_valid(&alias), "{alias}");
        assert_eq!(alias, lan_alias(&first.to_ascii_uppercase()));
        assert_ne!(alias, lan_alias(&second));
    }
}
