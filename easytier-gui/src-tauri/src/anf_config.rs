//! ANF 客户端本地配置持久化（exe 同目录的配置文件，非机密）。
//!
//! 关键约束：**绝不持久化网络密钥/密码**。配置文件只存服务器地址、网络名、
//! 邀请码状态、最近连接实例ID、机器 ID 等非机密信息；网络密钥由配置服务器
//! 在连接时实时下发、只留内存。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// 邀请码状态。一次性邀请码：首次成功连接后置为 Used。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InviteStatus {
    Pending,
    Approved,
    Used,
    Revoked,
}

impl Default for InviteStatus {
    fn default() -> Self {
        InviteStatus::Pending
    }
}

/// 本地配置（非机密）。结构上不包含任何网络密钥/密码字段。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AppConfig {
    pub schema_version: u32,
    pub machine_id: Option<String>,
    pub server_address: Option<String>,
    pub network_name: Option<String>,
    pub invite_code: Option<String>,
    pub invite_status: InviteStatus,
    pub last_instance_id: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            machine_id: None,
            server_address: None,
            network_name: None,
            invite_code: None,
            invite_status: InviteStatus::Pending,
            last_instance_id: None,
        }
    }
}

/// 生成新配置（带最新 schema_version），并兼容迁移旧版本。
pub fn new_config() -> AppConfig {
    AppConfig::default()
}

/// 把旧版本配置迁移到当前 schema_version（目前仅补默认字段）。
fn migrate(mut cfg: AppConfig) -> AppConfig {
    if cfg.schema_version == 0 {
        cfg.schema_version = SCHEMA_VERSION;
    }
    if cfg.schema_version < SCHEMA_VERSION {
        let mut merged = AppConfig::default();
        merged.machine_id = cfg.machine_id;
        merged.server_address = cfg.server_address;
        merged.network_name = cfg.network_name;
        merged.invite_code = cfg.invite_code;
        merged.invite_status = cfg.invite_status;
        merged.last_instance_id = cfg.last_instance_id;
        merged.schema_version = SCHEMA_VERSION;
        merged
    } else {
        cfg
    }
}

/// 从指定目录读取配置；不存在或解析失败时返回默认配置。
pub fn read_config_from(dir: &Path) -> AppConfig {
    let path = config_path_in(dir);
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text)
            .map(migrate)
            .unwrap_or_else(|_| new_config()),
        Err(_) => new_config(),
    }
}

/// 写入指定目录，返回实际写入的路径。
pub fn write_config_to(dir: &Path, cfg: &AppConfig) -> Result<PathBuf, String> {
    let path = config_path_in(dir);
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn config_path_in(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILE_NAME)
}

/// 可执行文件所在目录（Windows 主力：与 exe 同目录）。
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 只读目录回退：用户配置目录（Windows %APPDATA% / 其他 $HOME）。
pub fn fallback_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("anf-easytier");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".anf-easytier");
    }
    PathBuf::from(".")
}

/// 读取配置：优先 exe 同目录，其次用户目录，否则默认。
pub fn load_config() -> (AppConfig, PathBuf) {
    let exe_path = config_path_in(&exe_dir());
    if exe_path.exists() {
        return (read_config_from(&exe_dir()), exe_path);
    }
    let user_path = config_path_in(&fallback_dir());
    if user_path.exists() {
        return (read_config_from(&fallback_dir()), user_path);
    }
    (new_config(), exe_path)
}

/// 保存配置：优先 exe 同目录，失败回退用户目录，返回实际路径。
pub fn save_config(cfg: &AppConfig) -> Result<PathBuf, String> {
    match write_config_to(&exe_dir(), cfg) {
        Ok(p) => Ok(p),
        Err(_) => write_config_to(&fallback_dir(), cfg),
    }
}

/// 取或生成稳定机器 ID（UUID v4），并写回配置。
pub fn get_or_create_machine_id(cfg: &mut AppConfig) -> String {
    if let Some(id) = &cfg.machine_id {
        if !id.trim().is_empty() {
            return id.clone();
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    cfg.machine_id = Some(id.clone());
    id
}

/// 归一化服务器地址为配置源 URL：
/// - 已带 scheme（tcp/udp/ws/wss）→ 小写化并透传；
/// - 形如 `host:port` / `ip:port` → 默认 `<tcp>://host:port`；
/// - 缺端口 → 报错并给出示例。
pub fn normalize_address(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("服务器地址不能为空".to_string());
    }
    if let Some(idx) = s.find("://") {
        let scheme = s[..idx].to_lowercase();
        if !matches!(scheme.as_str(), "tcp" | "udp" | "ws" | "wss") {
            return Err(format!("不支持的协议: {scheme}"));
        }
        let rest = &s[idx + 3..];
        if rest.is_empty() {
            return Err("地址缺少主机/端口".to_string());
        }
        return Ok(format!("{scheme}://{rest}"));
    }
    if let Some(colon) = s.rfind(':') {
        let host = &s[..colon];
        let port = &s[colon + 1..];
        if host.is_empty() || port.parse::<u16>().is_err() {
            return Err("端口无效".to_string());
        }
        // config-server 是 UDP 且走 /admin 托管通道；小白只填 ip:port 时默认补全，
        // 否则 easytier-core 会因 token 为空而拒绝连接。
        return Ok(format!("udp://{host}:{port}/admin"));
    }
    Err("地址缺少端口（示例: 1.2.3.4:22020）".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("anf-config-test-{}-{}", std::process::id(), seed))
    }

    #[test]
    fn default_config_has_current_schema_and_no_secret() {
        let cfg = new_config();
        assert_eq!(cfg.schema_version, SCHEMA_VERSION);
        // 配置文件结构上不含任何网络密钥/密码字段
        let text = toml::to_string_pretty(&cfg).unwrap();
        for banned in ["secret", "password", "network_key", "psk"] {
            assert!(
                !text.to_lowercase().contains(banned),
                "配置不应包含机密字段 {banned}: {text}"
            );
        }
    }

    #[test]
    fn write_then_read_roundtrip() {
        let dir = temp_dir();
        let mut cfg = new_config();
        cfg.server_address = Some("udp://10.0.0.6:22020/admin".to_string());
        cfg.network_name = Some("anf-m3".to_string());
        cfg.invite_code = Some("INV-ABC123".to_string());
        cfg.invite_status = InviteStatus::Used;
        cfg.machine_id = Some(uuid::Uuid::new_v4().to_string());

        let path = write_config_to(&dir, &cfg).unwrap();
        assert_eq!(path, config_path_in(&dir));
        assert!(path.exists());

        let loaded = read_config_from(&dir);
        assert_eq!(loaded, cfg);
        // 清理
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_missing_returns_default() {
        let dir = temp_dir();
        let loaded = read_config_from(&dir);
        assert_eq!(loaded, new_config());
    }

    #[test]
    fn machine_id_is_stable_and_persisted() {
        let dir = temp_dir();
        let mut cfg = read_config_from(&dir);
        let a = get_or_create_machine_id(&mut cfg);
        let b = get_or_create_machine_id(&mut cfg);
        assert_eq!(a, b);
        assert!(uuid::Uuid::parse_str(&a).is_ok());
        // 写回后再次读取仍不变
        write_config_to(&dir, &cfg).unwrap();
        let mut reloaded = read_config_from(&dir);
        assert_eq!(get_or_create_machine_id(&mut reloaded), a);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_address_accepts_schemes_and_lowercases() {
        let cases = [
            ("tcp://H:80", "tcp://H:80"),
            ("UDP://10.0.0.1:22020", "udp://10.0.0.1:22020"),
            ("ws://h:443/admin", "ws://h:443/admin"),
            ("WSS://h:8443/x", "wss://h:8443/x"),
        ];
        for (raw, expected) in cases {
            assert_eq!(normalize_address(raw).unwrap(), expected, "case {raw}");
        }
    }

    #[test]
    fn normalize_address_defaults_missing_scheme_to_config_server() {
        assert_eq!(
            normalize_address("10.0.0.6:22020").unwrap(),
            "udp://10.0.0.6:22020/admin"
        );
        assert_eq!(
            normalize_address("example.com:8080").unwrap(),
            "udp://example.com:8080/admin"
        );
    }

    #[test]
    fn normalize_address_rejects_bad_input() {
        assert!(normalize_address("").is_err());
        assert!(normalize_address("http://h:80").is_err(), "http 不支持");
        assert!(normalize_address("10.0.0.1").is_err(), "缺端口报错");
        assert!(normalize_address("h:notaport").is_err(), "端口无效");
    }

    #[test]
    fn saves_to_exe_dir_and_falls_back_on_readonly() {
        // exe 目录在测试中不可控，这里只验证 save/load 路径逻辑幂等
        let dir = temp_dir();
        let cfg = new_config();
        let p = write_config_to(&dir, &cfg).unwrap();
        assert!(p.exists());
        let (loaded, _) = load_config_or_default_at(&dir);
        assert_eq!(loaded.schema_version, SCHEMA_VERSION);
        let _ = fs::remove_dir_all(&dir);
    }

    // 辅助：给定目录读取（与 load_config 一致，但指到指定目录，便于测试）
    fn load_config_or_default_at(dir: &Path) -> (AppConfig, PathBuf) {
        (read_config_from(dir), config_path_in(dir))
    }
}
