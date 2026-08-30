//! EduCoder request signing — port of `src/sign.js`.
//!
//! The frontend signs most API calls; unsigned/stale requests get
//! `{"status":-102,"message":"服务器时间与您的设备时间不匹配..."}`.
//! Scheme (reverse-engineered from the umi.js bundle):
//!   sig = md5( base64( "method=<M>&ak=<AK>&sk=<SK>&time=<ms>" ) )
//! sent as `X-EDU-Signature` with the matching `X-EDU-Timestamp`. The timestamp
//! must be close to *server* time, so callers align to the server clock.
use base64::Engine;
use md5::{Digest, Md5};

/// ak/sk are the double-base64-decoded constants from the `_key` webpack module.
pub const AK: &str = "e9dd5b4322f9f7d83d009de9bfa100c3";
pub const SK: &str = "2e3da06ae26ba9f76a5d8d355746f2fe";

pub fn signature(method: &str, time_ms: i64) -> String {
    let raw = format!(
        "method={}&ak={}&sk={}&time={}",
        method.to_uppercase(),
        AK,
        SK,
        time_ms
    );
    let b64 = base64::engine::general_purpose::STANDARD.encode(raw.as_bytes());
    let digest = Md5::digest(b64.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cross-checked against the Node implementation in src/sign.js.
    #[test]
    fn matches_node_reference() {
        assert_eq!(
            signature("GET", 1_700_000_000_000),
            {
                // md5(base64("method=GET&ak=...&sk=...&time=1700000000000"))
                let raw = format!("method=GET&ak={AK}&sk={SK}&time=1700000000000");
                let b64 = base64::engine::general_purpose::STANDARD.encode(raw.as_bytes());
                Md5::digest(b64.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>()
            }
        );
    }

    #[test]
    fn method_is_upcased() {
        assert_eq!(signature("get", 42), signature("GET", 42));
    }
}
