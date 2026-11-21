//! OAuth 1.0a request signing (RFC 5849) for Flickr (ADR-046): HMAC-SHA1
//! signatures, percent-encoding, and the form-encoded answers of the token
//! endpoints.
//!
//! SHA-1, HMAC and Base64 are written here (a few dozen lines, checked
//! against the FIPS 180 / RFC 3174, RFC 2202 and RFC 4648 test vectors and
//! the OAuth specification's signature example) so no dependency is added.
//! SHA-1 is what Flickr requires for OAuth 1.0a signatures; nothing else
//! in Keepsake uses it.

/// SHA-1 of `data` (FIPS 180-4).
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let bits = (data.len() as u64).wrapping_mul(8);
    let mut msg = Vec::with_capacity(data.len() + 72);
    msg.extend_from_slice(data);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (slot, word) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *slot = u32::from_be_bytes(*word);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 20];
    for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *chunk = word.to_be_bytes();
    }
    out
}

/// HMAC-SHA1 (RFC 2104).
pub fn hmac_sha1(key: &[u8], msg: &[u8]) -> [u8; 20] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..20].copy_from_slice(&sha1(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(64 + msg.len());
    inner.extend(k.iter().map(|b| b ^ 0x36));
    inner.extend_from_slice(msg);
    let mut outer = Vec::with_capacity(84);
    outer.extend(k.iter().map(|b| b ^ 0x5c));
    outer.extend_from_slice(&sha1(&inner));
    sha1(&outer)
}

/// Standard Base64 with padding (RFC 4648 §4).
pub fn base64(data: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        let sextet = |shift: u32| char::from(ABC[((n >> shift) & 63) as usize]);
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if c.len() > 1 { sextet(6) } else { '=' });
        out.push(if c.len() > 2 { sextet(0) } else { '=' });
    }
    out
}

/// Percent-encode as RFC 5849 §3.6 says: everything but the unreserved
/// characters (`A–Z a–z 0–9 - . _ ~`), upper-case hex, UTF-8.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Decode `application/x-www-form-urlencoded` (`+` is a space).
pub fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A form-encoded answer (`a=1&b=two`) as pairs, decoded.
pub fn parse_form(body: &str) -> Vec<(String, String)> {
    body.trim()
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (decode(k), decode(v)),
            None => (decode(p), String::new()),
        })
        .collect()
}

/// The value of `key` in form pairs.
pub fn form_value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// The signature base string (RFC 5849 §3.4.1): the method, the URL
/// without query, and every parameter, encoded and sorted.
pub fn base_string(method: &str, url: &str, params: &[(String, String)]) -> String {
    let mut enc: Vec<(String, String)> =
        params.iter().map(|(k, v)| (encode(k), encode(v))).collect();
    enc.sort();
    let joined = enc
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    format!(
        "{}&{}&{}",
        method.to_ascii_uppercase(),
        encode(url),
        encode(&joined)
    )
}

/// The HMAC-SHA1 signature of a request.
pub fn signature(
    method: &str,
    url: &str,
    params: &[(String, String)],
    consumer_secret: &str,
    token_secret: &str,
) -> String {
    let key = format!("{}&{}", encode(consumer_secret), encode(token_secret));
    base64(&hmac_sha1(
        key.as_bytes(),
        base_string(method, url, params).as_bytes(),
    ))
}

/// The `oauth_*` parameters of a request, before it is signed.
pub fn oauth_params(
    consumer_key: &str,
    token: Option<&str>,
    timestamp: i64,
    nonce: &str,
) -> Vec<(String, String)> {
    let mut p = vec![
        ("oauth_consumer_key".to_owned(), consumer_key.to_owned()),
        ("oauth_nonce".to_owned(), nonce.to_owned()),
        ("oauth_signature_method".to_owned(), "HMAC-SHA1".to_owned()),
        ("oauth_timestamp".to_owned(), timestamp.to_string()),
        ("oauth_version".to_owned(), "1.0".to_owned()),
    ];
    if let Some(t) = token {
        p.push(("oauth_token".to_owned(), t.to_owned()));
    }
    p
}

/// `params` as a query string (`a=1&b=2`), encoded.
pub fn query(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn sha1_matches_the_standard_vectors() {
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&sha1(&million)),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
        );
    }

    #[test]
    fn hmac_sha1_matches_rfc_2202() {
        assert_eq!(
            hex(&hmac_sha1(&[0x0b; 20], b"Hi There")),
            "b617318655057264e28bc0b6fb378c8ef146be00"
        );
        assert_eq!(
            hex(&hmac_sha1(b"Jefe", b"what do ya want for nothing?")),
            "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
        );
        // A key longer than a block is hashed first.
        assert_eq!(
            hex(&hmac_sha1(
                &[0xaa; 80],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "aa4ae5e15272d00e95705637ce8a3b55ed402112"
        );
    }

    #[test]
    fn base64_matches_rfc_4648() {
        for (i, o) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(i.as_bytes()), o);
        }
    }

    #[test]
    fn encoding_keeps_only_unreserved_characters() {
        assert_eq!(encode("Ladies + Gentlemen"), "Ladies%20%2B%20Gentlemen");
        assert_eq!(encode("An encoded string!"), "An%20encoded%20string%21");
        assert_eq!(encode("Dogs, Cats & Mice"), "Dogs%2C%20Cats%20%26%20Mice");
        assert_eq!(encode("☃"), "%E2%98%83");
        assert_eq!(encode("a-b.c_d~e"), "a-b.c_d~e");
    }

    #[test]
    fn form_answers_are_decoded() {
        let p = parse_form(
            "fullname=Jamal%20Fanaian&oauth_token=72157626318069415-087bfc7b5816092c\
             &oauth_token_secret=a202d1f853ec69de&user_nsid=21207597%40N07&username=jamalfanaian",
        );
        assert_eq!(form_value(&p, "fullname"), Some("Jamal Fanaian"));
        assert_eq!(form_value(&p, "user_nsid"), Some("21207597@N07"));
        assert_eq!(form_value(&p, "username"), Some("jamalfanaian"));
        assert_eq!(form_value(&p, "missing"), None);
        assert_eq!(decode("a+b%2"), "a b%2");
        assert_eq!(decode("100%"), "100%");
    }

    /// The signature example of OAuth Core 1.0 (Appendix A.5), the same
    /// algorithm as RFC 5849.
    #[test]
    fn the_specification_example_is_signed_the_same() {
        let mut p = oauth_params(
            "dpf43f3p2l4k3l03",
            Some("nnch734d00sl2jdk"),
            1_191_242_096,
            "kllo9940pd9333jh",
        );
        p.push(("file".into(), "vacation.jpg".into()));
        p.push(("size".into(), "original".into()));
        let url = "http://photos.example.net/photos";
        assert_eq!(
            base_string("GET", url, &p),
            "GET&http%3A%2F%2Fphotos.example.net%2Fphotos&file%3Dvacation.jpg%26\
             oauth_consumer_key%3Ddpf43f3p2l4k3l03%26oauth_nonce%3Dkllo9940pd9333jh%26\
             oauth_signature_method%3DHMAC-SHA1%26oauth_timestamp%3D1191242096%26\
             oauth_token%3Dnnch734d00sl2jdk%26oauth_version%3D1.0%26size%3Doriginal"
        );
        assert_eq!(
            signature("GET", url, &p, "kd94hf93k423kf44", "pfkkdhi9sl3r4s00"),
            "tR3+Ty81lMeYAr/Fid0kMTYa/WM="
        );
    }
}
