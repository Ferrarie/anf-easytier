//! ANF 用户 TOTP 两步验证核心逻辑：动态码生成/校验、secret 加密存储、
//! 账号级失败退避时长、主密钥加载。
//!
//! 设计共识（2026-08-29 与用户确认）：
//! - 仅 TOTP（SHA1 / 6 位 / 30 秒，±1 窗口），兼容所有验证器 App；
//! - secret 用主密钥 AES-256-GCM 加密后入库；
//! - 账号级退避：连续失败 5 次锁 10s → 30s → 指数翻倍，封顶 15 分钟；
//! - 不做恢复码，丢失验证器走服务端重置（CLI / 管理员后台）。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::{Context, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use data_encoding::{BASE32_NOPAD, HEXLOWER};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

type HmacSha1 = Hmac<Sha1>;

/// TOTP 参数：RFC 6238 标准配置
pub const TOTP_STEP_SECS: u64 = 30;
pub const TOTP_DIGITS: u32 = 6;
/// ±1 窗口容差（应对手机时钟小幅漂移）
pub const TOTP_SKEW: u64 = 1;
pub const TOTP_ISSUER: &str = "ANF";

/// 主密钥环境变量（任意非空字符串，sha256 后作为 32B AES 密钥）
pub const MASTER_KEY_ENV: &str = "ANF_TOTP_SECRET_KEY";

/// 账号级退避封顶：15 分钟
pub const MAX_LOCK_SECS: i64 = 900;
/// 触发一次锁定的连续失败次数
pub const FAILS_PER_LOCK: i64 = 5;

/// 当前 unix 秒
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// TOTP 当前窗口序号
pub fn current_step(unix_secs: i64) -> u64 {
    (unix_secs.max(0) as u64) / TOTP_STEP_SECS
}

/// RFC 4226 §5.3 动态截断：取摘要末字节低 4 位为偏移，拼 31 位整数
fn hotp_truncate(digest: &[u8]) -> u32 {
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let bin = ((digest[offset] as u32 & 0x7f) << 24)
        | ((digest[offset + 1] as u32) << 16)
        | ((digest[offset + 2] as u32) << 8)
        | (digest[offset + 3] as u32);
    bin
}

/// HOTP（RFC 4226）：返回 31 位动态截断原始值
pub fn hotp(secret: &[u8], counter: u64) -> u32 {
    let mut mac = <HmacSha1 as Mac>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    hotp_truncate(&digest)
}

/// 指定窗口序号的 6 位动态码（RFC 6238：HOTP 原始值对 10^6 取模）
pub fn totp_at(secret: &[u8], step: u64) -> String {
    format!(
        "{:0width$}",
        hotp(secret, step) % 10u32.pow(TOTP_DIGITS),
        width = TOTP_DIGITS as usize
    )
}

/// 生成随机 secret：20 字节 → base32 无填充（验证器手动录入用）
pub fn generate_secret() -> String {
    let mut bytes = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut bytes);
    BASE32_NOPAD.encode(&bytes)
}

fn decode_secret(secret_b32: &str) -> anyhow::Result<Vec<u8>> {
    BASE32_NOPAD
        .decode(secret_b32.trim().to_ascii_uppercase().as_bytes())
        .context("TOTP secret 不是合法的 base32")
}

/// 校验动态码。
///
/// 允许 ±[`TOTP_SKEW`] 窗口；匹配的窗口必须严格晚于 `last_step`（防同窗口重放）。
/// 返回 `Some(matched_step)` 表示匹配，`None` 表示不匹配。
pub fn verify_code(
    secret_b32: &str,
    code: &str,
    now_step: u64,
    last_step: Option<i64>,
) -> anyhow::Result<Option<u64>> {
    let code = code.trim();
    if code.len() != TOTP_DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let secret = decode_secret(secret_b32)?;
    for step in (now_step.saturating_sub(TOTP_SKEW))..=(now_step + TOTP_SKEW) {
        if last_step.is_some_and(|ls| step as i64 <= ls) {
            continue;
        }
        let candidate = totp_at(&secret, step);
        if candidate
            .as_bytes()
            .ct_eq(code.as_bytes())
            .into()
        {
            return Ok(Some(step));
        }
    }
    Ok(None)
}

/// AES-256-GCM 加密：输出 base64(12B nonce || ciphertext+tag)
pub fn encrypt_secret(key: &[u8; 32], plaintext: &str) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| anyhow::anyhow!("TOTP secret 加密失败"))?;
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(BASE64_STANDARD.encode(out))
}

/// [`encrypt_secret`] 的逆操作
pub fn decrypt_secret(key: &[u8; 32], encoded: &str) -> anyhow::Result<String> {
    let data = BASE64_STANDARD
        .decode(encoded.trim())
        .context("TOTP secret 密文不是合法 base64")?;
    if data.len() <= 12 {
        bail!("TOTP secret 密文长度非法");
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let cipher = Aes256Gcm::new(key.into());
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|_| anyhow::anyhow!("TOTP secret 解密失败（主密钥不匹配或密文损坏）"))?;
    String::from_utf8(plaintext).context("TOTP secret 解密结果不是合法 UTF-8")
}

/// 账号级退避时长：第 1 次锁 10s，第 2 次 30s，之后每次翻倍，封顶 [`MAX_LOCK_SECS`]。
///
/// `lock_round` 从 1 开始（第几轮锁定）。
pub fn lock_duration_secs(lock_round: i64) -> i64 {
    if lock_round <= 1 {
        return 10;
    }
    let mut secs = 30i64;
    for _ in 2..lock_round {
        secs = (secs.saturating_mul(2)).min(MAX_LOCK_SECS);
    }
    secs.min(MAX_LOCK_SECS)
}

/// 由累计失败次数计算锁定截止时间；不足 [`FAILS_PER_LOCK`] 次则不锁定。
pub fn lock_until_after_fail(fail_count: i64, now_ts: i64) -> Option<i64> {
    if fail_count > 0 && fail_count % FAILS_PER_LOCK == 0 {
        let round = fail_count / FAILS_PER_LOCK;
        Some(now_ts + lock_duration_secs(round))
    } else {
        None
    }
}

/// 生成 `otpauth://totp/...` URI（验证器扫码识别）
pub fn otpauth_uri(secret_b32: &str, issuer: &str, account: &str) -> String {
    let enc = |s: &str| {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(b as char)
                }
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    };
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits={}&period={}",
        enc(issuer),
        enc(account),
        secret_b32,
        enc(issuer),
        TOTP_DIGITS,
        TOTP_STEP_SECS
    )
}

/// 加载 TOTP 主密钥（32B）。
///
/// 优先级：环境变量 [`MASTER_KEY_ENV`]（任意非空字符串 sha256）→
/// DB 同目录 `<db_path>.totp_key` 文件（不存在则生成 32B 随机数，hex 存储）。
/// DB 备份单独泄露时不含密钥文件，secret 不直接暴露。
pub fn load_master_key(db_path: &str) -> anyhow::Result<[u8; 32]> {
    if let Ok(v) = std::env::var(MASTER_KEY_ENV) {
        if !v.trim().is_empty() {
            let digest = Sha256::digest(v.as_bytes());
            let mut key = [0u8; 32];
            key.copy_from_slice(&digest);
            return Ok(key);
        }
    }

    let key_path = format!("{}.totp_key", db_path);
    let path = Path::new(&key_path);
    if path.exists() {
        let hex = std::fs::read_to_string(path)
            .with_context(|| format!("读取 {key_path} 失败"))?;
        let bytes = HEXLOWER
            .decode(hex.trim().as_bytes())
            .with_context(|| format!("{key_path} 内容不是合法 hex"))?;
        return bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("{key_path} 密钥长度必须是 32 字节"));
    }

    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    std::fs::write(path, HEXLOWER.encode(&key))
        .with_context(|| format!("写入 {key_path} 失败"))?;
    tracing::info!("已生成 TOTP 主密钥文件 {key_path}（请随服务器一同备份，勿单独泄露）");
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 官方测试向量：secret 为 ASCII "12345678901234567890"
    const RFC_SECRET_B32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn rfc_secret() -> Vec<u8> {
        b"12345678901234567890".to_vec()
    }

    #[test]
    fn hotp_matches_rfc6238_vectors() {
        // 8 位对照：6 位码 = 同一 31 位截断值 % 1e6
        assert_eq!(hotp(&rfc_secret(), 59 / 30) % 100_000_000, 94287082);
        assert_eq!(hotp(&rfc_secret(), 1111111109 / 30) % 100_000_000, 7081804);
        assert_eq!(hotp(&rfc_secret(), 1111111111 / 30) % 100_000_000, 14050471);
        assert_eq!(hotp(&rfc_secret(), 1234567890 / 30) % 100_000_000, 89005924);
        assert_eq!(hotp(&rfc_secret(), 2000000000 / 30) % 100_000_000, 69279037);
    }

    #[test]
    fn totp_at_formats_six_digits() {
        assert_eq!(totp_at(&rfc_secret(), 59 / 30), "287082");
        assert_eq!(totp_at(&rfc_secret(), 1111111109 / 30), "081804");
        assert_eq!(totp_at(&rfc_secret(), 1234567890 / 30), "005924");
    }

    #[test]
    fn verify_code_accepts_current_and_previous_window() {
        let now = 1000;
        let current = totp_at(&rfc_secret(), now);
        assert_eq!(
            verify_code(RFC_SECRET_B32, &current, now, None).unwrap(),
            Some(now)
        );
        // 手机慢半拍：上一窗口的码仍在容差内
        let previous = totp_at(&rfc_secret(), now - 1);
        assert_eq!(
            verify_code(RFC_SECRET_B32, &previous, now, None).unwrap(),
            Some(now - 1)
        );
    }

    #[test]
    fn verify_code_rejects_wrong_and_replayed_code() {
        let now = 1000;
        assert_eq!(
            verify_code(RFC_SECRET_B32, "000000", now, None).unwrap(),
            None
        );
        // 重放：上一窗口的码已被记录为 last_step，必须拒绝
        let previous = totp_at(&rfc_secret(), now - 1);
        assert_eq!(
            verify_code(RFC_SECRET_B32, &previous, now, Some((now - 1) as i64)).unwrap(),
            None
        );
    }

    #[test]
    fn verify_code_rejects_malformed_input() {
        let now = 1000;
        // 长度不对 / 非数字 / 非法 base32
        assert_eq!(verify_code(RFC_SECRET_B32, "12345", now, None).unwrap(), None);
        assert_eq!(verify_code(RFC_SECRET_B32, "12a456", now, None).unwrap(), None);
        assert!(verify_code("!!not-base32!!", "123456", now, None).is_err());
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [7u8; 32];
        let secret = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";
        let enc = encrypt_secret(&key, secret).unwrap();
        assert_ne!(enc, secret);
        assert_eq!(decrypt_secret(&key, &enc).unwrap(), secret);
        // 密文随机化：同一明文两次加密结果不同（nonce 随机）
        assert_ne!(encrypt_secret(&key, secret).unwrap(), enc);
    }

    #[test]
    fn decrypt_fails_with_wrong_key_or_tampered_ciphertext() {
        let enc = encrypt_secret(&[1u8; 32], "hello-secret").unwrap();
        assert!(decrypt_secret(&[2u8; 32], &enc).is_err());
        let mut tampered = enc.clone();
        tampered.replace_range(20..21, if &enc[20..21] == "A" { "B" } else { "A" });
        assert!(decrypt_secret(&[1u8; 32], &tampered).is_err());
    }

    #[test]
    fn lock_duration_follows_agreed_ladder() {
        assert_eq!(lock_duration_secs(1), 10);
        assert_eq!(lock_duration_secs(2), 30);
        assert_eq!(lock_duration_secs(3), 60);
        assert_eq!(lock_duration_secs(4), 120);
        assert_eq!(lock_duration_secs(6), 480);
        assert_eq!(lock_duration_secs(7), 900);
        assert_eq!(lock_duration_secs(100), 900);
        assert_eq!(lock_duration_secs(0), 10);
    }

    #[test]
    fn lock_until_triggers_every_five_fails() {
        assert_eq!(lock_until_after_fail(4, 1000), None);
        assert_eq!(lock_until_after_fail(5, 1000), Some(1010));
        assert_eq!(lock_until_after_fail(9, 1000), None);
        assert_eq!(lock_until_after_fail(10, 1000), Some(1030));
        assert_eq!(lock_until_after_fail(15, 1000), Some(1060));
        assert_eq!(lock_until_after_fail(0, 1000), None);
    }

    #[test]
    fn otpauth_uri_contains_required_params() {
        let uri = otpauth_uri("JBSWY3DP", "ANF", "admin");
        assert!(uri.starts_with("otpauth://totp/ANF:admin?"));
        assert!(uri.contains("secret=JBSWY3DP"));
        assert!(uri.contains("issuer=ANF"));
        assert!(uri.contains("algorithm=SHA1"));
        assert!(uri.contains("digits=6"));
        assert!(uri.contains("period=30"));
        // 账号名特殊字符需 percent-encode
        let uri2 = otpauth_uri("JBSWY3DP", "ANF", "a b@c");
        assert!(uri2.contains("ANF:a%20b%40c"));
    }

    #[test]
    fn generate_secret_is_base32_of_20_bytes() {
        let s = generate_secret();
        assert_eq!(s.len(), 32); // 20 字节 → 160 bit → base32 无填充 32 字符
        assert!(BASE32_NOPAD.decode(s.as_bytes()).is_ok());
        assert_ne!(generate_secret(), s);
    }

    #[test]
    fn master_key_env_takes_precedence_then_file_is_reused() {
        // 1) env 优先：任意字符串 sha256，不触碰 DB 文件
        unsafe { std::env::set_var(MASTER_KEY_ENV, "anf-test-key") };
        let k = load_master_key("Z:/anf-nonexistent-dir/test.db").unwrap();
        let expect: [u8; 32] = Sha256::digest(b"anf-test-key").into();
        assert_eq!(k, expect);
        unsafe { std::env::remove_var(MASTER_KEY_ENV) };

        // 2) 无 env：key 文件自动生成且复用
        let dir = std::env::temp_dir().join(format!("anf-totp-key-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test.db");
        let db_path = db_path.to_str().unwrap();
        let k1 = load_master_key(db_path).unwrap();
        let k2 = load_master_key(db_path).unwrap();
        assert_eq!(k1, k2);
        assert!(dir.join("test.db.totp_key").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
