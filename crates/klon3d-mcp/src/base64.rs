//! Standard base64 with padding (RFC 4648), for the PNG of `render_preview`.

const ZEICHEN: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode `daten` as base64 with `=` padding.
pub fn kodieren(daten: &[u8]) -> String {
    let mut out = String::with_capacity(daten.len().div_ceil(3) * 4);
    for block in daten.chunks(3) {
        let b = [
            block[0],
            *block.get(1).unwrap_or(&0),
            *block.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= block.len() {
                out.push(ZEICHEN[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decode standard base64; `None` on a character outside the alphabet or a wrong length.
#[cfg(test)]
pub fn dekodieren(text: &str) -> Option<Vec<u8>> {
    let text = text.as_bytes();
    if text.len() % 4 != 0 {
        return None;
    }
    let wert = |c: u8| ZEICHEN.iter().position(|&z| z == c).map(|p| p as u32);
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for block in text.chunks(4) {
        let fuell = block.iter().rev().take_while(|&&c| c == b'=').count();
        let mut n = 0u32;
        for &c in &block[..4 - fuell] {
            n = (n << 6) | wert(c)?;
        }
        n <<= 6 * fuell as u32;
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - fuell]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4648_beispiele() {
        let paare = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (klar, kodiert) in paare {
            assert_eq!(kodieren(klar.as_bytes()), kodiert);
            assert_eq!(dekodieren(kodiert).unwrap(), klar.as_bytes());
        }
    }

    #[test]
    fn alle_bytes_hin_und_zurueck() {
        let daten: Vec<u8> = (0..=255u8).chain(0..=7).collect();
        assert_eq!(dekodieren(&kodieren(&daten)).unwrap(), daten);
    }
}
