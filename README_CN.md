# ANF Platform (ANF EasyTier)

> This is the **English** companion to the primary doc [README.md](/README.md).
> Please read `README.md` (Simplified Chinese) for the authoritative ANFAGENT-30 description.

[简体中文](/README.md) | [English](/README_CN.md)

## TL;DR

`ANF Platform` is a **centralized group-networking** fork of [EasyTier](https://github.com/EasyTier/EasyTier).
Instead of the decentralized "peer-to-peer" model of upstream EasyTier, nodes must be **approved by an admin**
before they can join the network:

- **Center**: `easytier-web` (device approval / network & ACL management) + center `easytier-core` (relay / fallback).
- **Device**: a stable `machine_id` (derived from NIC + CPU) is the unit of authorization; a device registers with
  an invitation code and joins only after an admin approves it.
- **Network**: network instances are hard-isolated; the TUN device is named `anf_et`; tag / ACL rules default to
  `drop` unless explicitly allowed.

## Naming Convention

Release artifacts follow: `anf_<version>_<platform>_<arch>.zip`

```text
anf_2.6.4_windows_x64.zip
anf_2.6.4_macos_arm64.zip
anf_2.6.4_linux_x86_64.zip
```

Version `2.6.4` matches the underlying core. Deprecated names such as `anf-easytier-win-x64-2.6.4-anf.1`
and `anf_平台架构_2.6.4_windows_x64` are replaced by this convention.

## Key Differences vs. Upstream EasyTier

| Dimension | Upstream EasyTier | ANF Platform |
| --- | --- | --- |
| Authorization unit | network_name + network_secret | stable machine_id (NIC + CPU) |
| Onboarding | provide the same params manually | invitation code → admin approval |
| Config source | local toml / CLI | centralized managed config |
| Network security | depends on network_secret | hard-isolated instances + tag / ACL deny-by-default |
| Permissions | equal peers | centralized admin control |

## Windows GUI

- **ANF Quick Connect**: save multiple center server profiles (auto-saved), auto-refill the last successful config.
- **Machine ID**: read-only, stable per machine; admins use it for approval.
- **Admin privilege required**: to create the TUN virtual NIC on Windows; a hint is shown when running as non-admin.
- **Client-only mode**: the GUI keeps only the client (`normal`) mode; server / remote modes are removed.

## Security Model

- `machine_id` is the stable authorization unit derived from hardware.
- Invitation-code registration + admin approval.
- Hard-isolated network instances with deny-by-default ACLs.
- `network_secret` is used for **membership proof** (HMAC-SHA256), not as the data-encryption key;
  link encryption is handled by EasyTier AES-GCM / WireGuard.
- config-server control channel upgrades to a `Noise_NN_25519_ChaChaPoly_SHA256` secure tunnel (AES-GCM)
  when supported; see the Chinese doc for the fallback risk.

## License

Same as upstream EasyTier: [LGPL-3.0](https://github.com/EasyTier/EasyTier/blob/main/LICENSE).
