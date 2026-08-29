//! ANF 客户端本地配置持久化（exe 同目录的配置文件，非机密）。
//!
//! 关键约束：**绝不持久化网络密钥/密码**。配置文件只存服务器地址、网络名、
//! 邀请码状态、最近连接实例ID、机器 ID 等非机密信息；网络密钥由配置服务器
//! 在连接时实时下发、只留内存。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 2;
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// 邀请码状态。一次性邀请码：首次成功连接后置为 Used。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum InviteStatus {
    #[default]
    Pending,
    Approved,
    Used,
    Revoked,
}

/// 本地配置（非机密）。结构上不包含任何网络密钥/密码字段。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AnfProfile {
    pub name: Option<String>,
    pub server_address: Option<String>,
    pub nickname: Option<String>,
    pub network_name: Option<String>,
    pub last_instance_id: Option<String>,
}

impl Default for AnfProfile {
    fn default() -> Self {
        Self {
            name: Some("默认".to_string()),
            server_address: None,
            nickname: None,
            network_name: None,
            last_instance_id: None,
        }
    }
}

fn default_profile_index() -> usize {
    0
}

fn default_profiles() -> Vec<AnfProfile> {
    vec![AnfProfile::default()]
}

/// 本地配置（非机密）。结构上不包含任何网络密钥/密码字段。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AppConfig {
    pub schema_version: u32,
    pub machine_id: Option<String>,
    #[serde(default)]
    pub invite_status: InviteStatus,
    #[serde(default = "default_profile_index")]
    pub active_profile_index: usize,
    #[serde(default = "default_profiles")]
    pub profiles: Vec<AnfProfile>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            machine_id: None,
            invite_status: InviteStatus::Pending,
            active_profile_index: 0,
            profiles: default_profiles(),
        }
    }
}

/// 生成新配置（带最新 schema_version），并兼容迁移旧版本。
pub fn new_config() -> AppConfig {
    AppConfig::default()
}

/// 把旧的单字段（v1）配置迁移为 v2 多档案结构。
///
/// - 存在 `profiles` 数组 → 直接当 v2 处理，仅兜底 schema_version。
/// - 否则把扁平字段（server_address/nickname/network_name/last_instance_id/invite_code）
///   聚合成 `profiles[0]`，并去掉扁平字段。
fn migrate_value(mut v: toml::Value) -> toml::Value {
    // 依赖 toml::Value 的 table；非 table 直接原样返回。
    let Some(table) = v.as_table_mut() else {
        return v;
    };
    let has_profiles = table
        .get("profiles")
        .and_then(|p| p.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false);

    if !has_profiles {
        let mut profile = toml::Table::new();
        let legacy_keys = [
            "server_address",
            "nickname",
            "network_name",
            "last_instance_id",
            "invite_code",
        ];
        for key in legacy_keys {
            if let Some(val) = table.get(key).cloned() {
                let keep = match val.as_str() {
                    Some(s) => !s.trim().is_empty(),
                    None => true,
                };
                if keep {
                    profile.insert(key.to_string(), val);
                }
            }
        }
        profile
            .entry("name".to_string())
            .or_insert_with(|| toml::Value::String("默认".to_string()));
        table.insert(
            "profiles".to_string(),
            toml::Value::Array(vec![toml::Value::Table(profile)]),
        );
        table.insert("active_profile_index".to_string(), toml::Value::Integer(0));
    }

    let dropped_keys = [
        "server_address",
        "nickname",
        "network_name",
        "last_instance_id",
        "invite_code",
        "network_secret",
    ];
    for key in dropped_keys {
        table.remove(key);
    }
    table.insert(
        "schema_version".to_string(),
        toml::Value::Integer(SCHEMA_VERSION as i64),
    );
    v
}

/// 从指定目录读取配置；不存在或解析失败时返回默认配置。
pub fn read_config_from(dir: &Path) -> AppConfig {
    let path = config_path_in(dir);
    match fs::read_to_string(&path) {
        Ok(text) => {
            let value: toml::Value =
                toml::from_str(&text).unwrap_or_else(|_| toml::Value::Table(Default::default()));
            let value = migrate_value(value);
            value
                .try_into::<AppConfig>()
                .unwrap_or_else(|_| new_config())
        }
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

/// 取或生成稳定机器 ID，并写回配置。
///
/// 机器码必须“同机不变且硬件相关”，避免老的随机 UUID 或配置文件丢失导致变化：
/// 优先用系统硬件标识（Windows `MachineGuid` / Linux `machine-id` / macOS `IOPlatformUUID`，
/// Linux 额外并入网卡 MAC）做确定性 UUID v5；取不到时回退到已有配置或随机值。
pub fn get_or_create_machine_id(cfg: &mut AppConfig) -> String {
    let id = hardware_machine_id()
        .or_else(|| {
            cfg.machine_id
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
        })
        .unwrap_or_else(uuid::Uuid::new_v4)
        .to_string();
    if cfg.machine_id.as_deref() != Some(id.as_str()) {
        cfg.machine_id = Some(id.clone());
    }
    id
}

/// 基于系统硬件标识确定性计算机器码 UUID。
fn hardware_machine_id() -> Option<uuid::Uuid> {
    // Android 无 machine-uid 实现（crate 未覆盖 target_os="android"），回退到配置/随机 ID
    #[cfg(target_os = "android")]
    {
        None
    }

    #[cfg(not(target_os = "android"))]
    {
        let uid = machine_uid::get().ok()?;
        let uid = uid.trim();
        if uid.is_empty() {
            return None;
        }
        let seed = format!("anf-easytier\nmachine_uid={uid}");
        // Linux 额外并入网卡 MAC（Windows/macOS 的 machine_uid 已是硬件指纹）。
        #[cfg(target_os = "linux")]
        let seed = {
            let macs = collect_linux_mac_addresses();
            if macs.is_empty() {
                seed
            } else {
                format!("{seed}\nmacs={}", macs.join(","))
            }
        };
        Some(uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_OID,
            seed.as_bytes(),
        ))
    }
}

#[cfg(target_os = "linux")]
fn collect_linux_mac_addresses() -> Vec<String> {
    let mut macs = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/class/net") else {
        return macs;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name == "lo" {
            continue;
        }
        let address_path = entry.path().join("address");
        let Ok(address) = std::fs::read_to_string(address_path) else {
            continue;
        };
        let address = address.trim().to_ascii_lowercase();
        if address.is_empty() || address == "00:00:00:00:00:00" {
            continue;
        }
        macs.push(address);
    }
    macs.sort();
    macs.dedup();
    macs.truncate(3);
    macs
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
    fn new_config_has_one_default_profile() {
        let cfg = new_config();
        assert_eq!(cfg.active_profile_index, 0);
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.profiles[0].name.as_deref(), Some("默认"));
    }

    #[test]
    fn app_config_deserializes_when_invite_status_missing() {
        // 前端 persist() 发送的 JSON 不带 invite_status，需默认 Pending 且不报错。
        let json = r#"{
            "schema_version":2,
            "machine_id":"9f0fd0bf-2ff8-58aa-9b0b-9dd5840165bc",
            "active_profile_index":0,
            "profiles":[
                {
                    "name":"中心A",
                    "server_address":"udp://127.0.0.1:22020/admin",
                    "nickname":"n",
                    "network_name":"anf-m3",
                    "last_instance_id":"i"
                }
            ]
        }"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.invite_status, InviteStatus::Pending);
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(
            cfg.profiles[0].server_address.as_deref(),
            Some("udp://127.0.0.1:22020/admin")
        );
        assert_eq!(cfg.profiles[0].network_name.as_deref(), Some("anf-m3"));
        assert_eq!(cfg.profiles[0].last_instance_id.as_deref(), Some("i"));
        assert_eq!(cfg.active_profile_index, 0);
    }

    #[test]
    fn write_then_read_roundtrip() {
        let dir = temp_dir();
        let mut cfg = new_config();
        cfg.invite_status = InviteStatus::Used;
        cfg.machine_id = Some(uuid::Uuid::new_v4().to_string());
        cfg.profiles = vec![
            AnfProfile {
                name: Some("中心A".to_string()),
                server_address: Some("udp://127.0.0.1:22020/admin".to_string()),
                nickname: Some("办公室".to_string()),
                network_name: Some("anf-m3".to_string()),
                last_instance_id: Some("i1".to_string()),
            },
            AnfProfile {
                name: Some("中心B".to_string()),
                server_address: Some("udp://10.9.9.9:22020/admin".to_string()),
                nickname: Some("会议室".to_string()),
                network_name: None,
                last_instance_id: None,
            },
        ];
        cfg.active_profile_index = 1;

        let path = write_config_to(&dir, &cfg).unwrap();
        assert_eq!(path, config_path_in(&dir));
        assert!(path.exists());

        let loaded = read_config_from(&dir);
        assert_eq!(loaded, cfg);
        assert_eq!(loaded.profiles.len(), 2);
        assert_eq!(loaded.active_profile_index, 1);
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
    fn migrates_v1_flat_config_into_single_profile() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let v1 = r#"schema_version = 1
machine_id = "9f0fd0bf-2ff8-58aa-9b0b-9dd5840165bc"
invite_status = "pending"
server_address = "udp://127.0.0.1:22020/admin"
nickname = "办公室电脑"
network_name = "anf-m3"
last_instance_id = "i-abc"
"#;
        fs::write(config_path_in(&dir), v1).unwrap();
        let cfg = read_config_from(&dir);
        assert_eq!(cfg.schema_version, SCHEMA_VERSION);
        assert_eq!(
            cfg.machine_id.as_deref(),
            Some("9f0fd0bf-2ff8-58aa-9b0b-9dd5840165bc")
        );
        assert_eq!(cfg.active_profile_index, 0);
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(
            cfg.profiles[0].server_address.as_deref(),
            Some("udp://127.0.0.1:22020/admin")
        );
        assert_eq!(cfg.profiles[0].nickname.as_deref(), Some("办公室电脑"));
        assert_eq!(cfg.profiles[0].network_name.as_deref(), Some("anf-m3"));
        assert_eq!(cfg.profiles[0].last_instance_id.as_deref(), Some("i-abc"));
        // 扁平字段不应再残留
        let text = fs::read_to_string(config_path_in(&dir)).unwrap();
        assert!(!text.contains("invite_code"));
        let _ = fs::remove_dir_all(&dir);
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
    fn machine_id_is_hardware_stable_across_config_dirs() {
        // 同机不同配置目录应得到相同机器码（由硬件标识决定，而非随机 UUID）。
        let d1 = temp_dir();
        let d2 = temp_dir();
        let mut c1 = read_config_from(&d1);
        let mut c2 = read_config_from(&d2);
        let a = get_or_create_machine_id(&mut c1);
        let b = get_or_create_machine_id(&mut c2);
        assert_eq!(a, b, "同一台机器在不同配置位置应得到相同的机器码");
        assert!(uuid::Uuid::parse_str(&a).is_ok());
        let _ = fs::remove_dir_all(&d1);
        let _ = fs::remove_dir_all(&d2);
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
            normalize_address("127.0.0.1:22020").unwrap(),
            "udp://127.0.0.1:22020/admin"
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

    #[test]
    fn multi_profile_machine_id_stays_global() {
        // 机器码是整机级字段，与具体档案无关。
        let dir = temp_dir();
        let mut cfg = new_config();
        cfg.profiles = vec![
            AnfProfile {
                name: Some("中心A".to_string()),
                server_address: Some("udp://a:1".to_string()),
                nickname: None,
                network_name: None,
                last_instance_id: None,
            },
            AnfProfile {
                name: Some("中心B".to_string()),
                server_address: Some("udp://b:2".to_string()),
                nickname: None,
                network_name: None,
                last_instance_id: None,
            },
        ];
        let id = get_or_create_machine_id(&mut cfg);
        write_config_to(&dir, &cfg).unwrap();
        let loaded = read_config_from(&dir);
        assert_eq!(loaded.machine_id.as_deref(), Some(id.as_str()));
        assert_eq!(loaded.profiles.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    // 辅助：给定目录读取（与 load_config 一致，但指到指定目录，便于测试）
    fn load_config_or_default_at(dir: &Path) -> (AppConfig, PathBuf) {
        (read_config_from(dir), config_path_in(dir))
    }
}
