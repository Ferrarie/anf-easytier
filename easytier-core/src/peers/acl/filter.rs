use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::atomic::Ordering;
use std::{
    net::IpAddr,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use easytier_proto::acl::{Acl, AclStats, Action, ChainType, Protocol};
use quanta::Instant;
use tokio_util::task::AbortOnDropHandle;

use crate::{
    packet::{PacketType, ZCPacket},
    peers::acl::processor::{AclProcessor, AclResult, AclStatKey, AclStatType, PacketInfo},
};

const IP_PROTO_ICMP: u8 = 1;
const IP_PROTO_TCP: u8 = 6;
const IP_PROTO_UDP: u8 = 17;
const IP_PROTO_ICMPV6: u8 = 58;

#[derive(Clone, Copy)]
struct ParsedIpPacket<'a> {
    src_ip: IpAddr,
    dst_ip: IpAddr,
    protocol: u8,
    transport_payload: &'a [u8],
}

fn parse_ip_packet(payload: &[u8]) -> Option<ParsedIpPacket<'_>> {
    let version = payload.first()? >> 4;
    match version {
        4 => parse_ipv4_packet(payload),
        6 => parse_ipv6_packet(payload),
        _ => None,
    }
}

fn parse_ipv4_packet(payload: &[u8]) -> Option<ParsedIpPacket<'_>> {
    if payload.len() < 20 {
        return None;
    }
    let header_len = usize::from(payload[0] & 0x0f) * 4;
    let options_len = header_len.saturating_sub(20);
    let payload_offset = 20 + options_len;
    let payload_start = payload_offset.min(payload.len());
    let total_length = usize::from(u16::from_be_bytes([payload[2], payload[3]]));
    let payload_len = total_length.saturating_sub(header_len);
    let payload_end = payload_start.saturating_add(payload_len).min(payload.len());

    Some(ParsedIpPacket {
        src_ip: IpAddr::V4(Ipv4Addr::new(
            payload[12],
            payload[13],
            payload[14],
            payload[15],
        )),
        dst_ip: IpAddr::V4(Ipv4Addr::new(
            payload[16],
            payload[17],
            payload[18],
            payload[19],
        )),
        protocol: payload[9],
        transport_payload: &payload[payload_start..payload_end],
    })
}

fn parse_ipv6_packet(payload: &[u8]) -> Option<ParsedIpPacket<'_>> {
    if payload.len() < 40 {
        return None;
    }
    let payload_len = usize::from(u16::from_be_bytes([payload[4], payload[5]]));
    let payload_end = 40usize.saturating_add(payload_len).min(payload.len());

    Some(ParsedIpPacket {
        src_ip: IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(&payload[8..24]).ok()?)),
        dst_ip: IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(&payload[24..40]).ok()?)),
        protocol: payload[6],
        transport_payload: &payload[40..payload_end],
    })
}

fn parse_transport_ports(protocol: u8, payload: &[u8]) -> Option<(Option<u16>, Option<u16>)> {
    let min_len = match protocol {
        IP_PROTO_TCP => 20,
        IP_PROTO_UDP => 8,
        _ => return Some((None, None)),
    };
    if payload.len() < min_len {
        return None;
    }

    Some((
        Some(u16::from_be_bytes([payload[0], payload[1]])),
        Some(u16::from_be_bytes([payload[2], payload[3]])),
    ))
}

fn acl_protocol(protocol: u8) -> Protocol {
    match protocol {
        IP_PROTO_TCP => Protocol::Tcp,
        IP_PROTO_UDP => Protocol::Udp,
        IP_PROTO_ICMP => Protocol::Icmp,
        IP_PROTO_ICMPV6 => Protocol::IcmPv6,
        _ => Protocol::Unspecified,
    }
}

#[derive(Debug, Eq, PartialEq, Hash)]
struct OutboundAllowRecord {
    src_ip: IpAddr,
    dst_ip: IpAddr,
    src_port: Option<u16>,
    dst_port: Option<u16>,
    protocol: Protocol,
}

impl OutboundAllowRecord {
    fn new_from_inbound_packet(p: &PacketInfo) -> Self {
        Self {
            src_ip: p.src_ip,
            dst_ip: p.dst_ip,
            src_port: p.src_port,
            dst_port: p.dst_port,
            protocol: p.protocol,
        }
    }

    fn new_from_outbound_packet(p: &PacketInfo) -> Self {
        Self {
            src_ip: p.dst_ip,
            dst_ip: p.src_ip,
            src_port: p.dst_port,
            dst_port: p.src_port,
            protocol: p.protocol,
        }
    }
}

/// ACL filter that can be inserted into the packet processing pipeline
/// Optimized with lock-free hot reloading via atomic processor replacement
pub struct AclFilter {
    // Use ArcSwap for lock-free atomic replacement during hot reload
    acl_processor: ArcSwap<AclProcessor>,
    acl_enabled: Arc<AtomicBool>,

    // Track allowed outbound packets and automatically allow their corresponding inbound response
    // packets, even if they would normally be dropped by ACL rules
    outbound_allow_records: Arc<DashMap<OutboundAllowRecord, Instant>>,
    #[allow(dead_code)]
    clean_task: Mutex<Option<AbortOnDropHandle<()>>>,
}

impl Default for AclFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl AclFilter {
    pub fn new() -> Self {
        let outbound_allow_records = Arc::new(DashMap::new());
        let record_clone = outbound_allow_records.clone();
        Self {
            acl_processor: ArcSwap::from(Arc::new(AclProcessor::new(Acl::default()))),
            acl_enabled: Arc::new(AtomicBool::new(false)),
            outbound_allow_records,
            clean_task: Mutex::new(Some(AbortOnDropHandle::new(tokio::spawn(async move {
                let max_life = std::time::Duration::from_secs(30);
                loop {
                    record_clone.retain(|_, v| v.elapsed() < max_life);
                    crate::foundation::time::sleep(std::time::Duration::from_secs(30)).await;
                }
            })))),
        }
    }

    pub(crate) async fn stop_cleanup_task(&self) {
        let task = self.clean_task.lock().unwrap().take();
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
    }

    /// Hot reload ACL rules by creating a new processor instance
    /// Preserves connection tracking and rate limiting state across reloads
    /// Now lock-free and doesn't require &mut self!
    pub fn reload_rules(&self, acl_config: Option<&Acl>) {
        self.outbound_allow_records.clear();

        let Some(acl_config) = acl_config else {
            self.acl_enabled.store(false, Ordering::Relaxed);
            return;
        };

        // Get current processor to extract shared state
        let current_processor = self.acl_processor.load();
        let (conn_track, rate_limiters, stats) = current_processor.get_shared_state();

        // Create new processor with preserved state
        let new_processor = AclProcessor::new_with_shared_state(
            acl_config.clone(),
            Some(conn_track),
            Some(rate_limiters),
            Some(stats),
        );

        // Atomic replacement - this is completely lock-free!
        self.acl_processor.store(Arc::new(new_processor));
        self.acl_enabled.store(true, Ordering::Relaxed);

        tracing::info!("ACL rules hot reloaded with preserved state (lock-free)");
    }

    /// Get current processor for processing packets
    pub fn get_processor(&self) -> Arc<AclProcessor> {
        self.acl_processor.load_full()
    }

    pub fn get_stats(&self) -> AclStats {
        let processor = self.get_processor();
        let global_stats = processor.get_stats();
        let (conn_track, _, _) = processor.get_shared_state();
        let rules_stats = processor.get_rules_stats();

        AclStats {
            global: global_stats.into_iter().collect(),
            conn_track: conn_track.iter().map(|x| *x.value()).collect(),
            rules: rules_stats,
        }
    }

    /// Extract packet information for ACL processing
    fn extract_packet_info(
        &self,
        packet: &ZCPacket,
        route: &(dyn crate::peers::route::Route + Send + Sync + 'static),
    ) -> Option<PacketInfo> {
        let payload = packet.payload();

        let parsed = parse_ip_packet(payload)?;
        let (src_port, dst_port) =
            parse_transport_ports(parsed.protocol, parsed.transport_payload)?;
        let acl_protocol = acl_protocol(parsed.protocol);

        let src_groups = packet
            .get_src_peer_id()
            .map(|peer_id| route.get_peer_groups(peer_id))
            .unwrap_or_else(|| Arc::new(Vec::new()));
        // 出站报文在 ACL 检查时目的 peer 尚未路由（to_peer_id=0），
        // 此时按目标 IP 解析对端组，保证 destination_groups 规则可匹配。
        let dst_groups = match packet.get_dst_peer_id() {
            Some(peer_id) if peer_id != 0 => route.get_peer_groups(peer_id),
            _ => route.get_peer_groups_by_ip_sync(&parsed.dst_ip),
        };

        Some(PacketInfo {
            src_ip: parsed.src_ip,
            dst_ip: parsed.dst_ip,
            src_port,
            dst_port,
            protocol: acl_protocol,
            packet_size: payload.len(),
            src_groups,
            dst_groups,
        })
    }

    /// Process ACL result and log if needed
    pub fn handle_acl_result(
        &self,
        result: &AclResult,
        packet_info: &PacketInfo,
        chain_type: ChainType,
        processor: &AclProcessor,
    ) {
        if result.should_log
            && let Some(ref log_context) = result.log_context
        {
            let log_message = log_context.to_message();
            tracing::info!(
                src_ip = %packet_info.src_ip,
                dst_ip = %packet_info.dst_ip,
                src_port = packet_info.src_port,
                dst_port = packet_info.dst_port,
                src_group = packet_info.src_groups.join(","),
                dst_group = packet_info.dst_groups.join(","),
                protocol = ?packet_info.protocol,
                action = ?result.action,
                rule = result.matched_rule_str().as_deref().unwrap_or("unknown"),
                chain_type = ?chain_type,
                "ACL: {}", log_message
            );
        }

        // Update global statistics in the ACL processor
        match result.action {
            Action::Allow => {
                processor.increment_stat(AclStatKey::PacketsAllowed);
                processor.increment_stat(AclStatKey::from_chain_and_action(
                    chain_type,
                    AclStatType::Allowed,
                ));
                tracing::trace!("ACL: Packet allowed");
            }
            Action::Drop => {
                processor.increment_stat(AclStatKey::PacketsDropped);
                processor.increment_stat(AclStatKey::from_chain_and_action(
                    chain_type,
                    AclStatType::Dropped,
                ));
                tracing::debug!("ACL: Packet dropped");
            }
            Action::Noop => {
                processor.increment_stat(AclStatKey::PacketsNoop);
                processor.increment_stat(AclStatKey::from_chain_and_action(
                    chain_type,
                    AclStatType::Noop,
                ));
                tracing::trace!("ACL: No operation");
            }
        }

        // Track total packets processed per chain
        processor.increment_stat(AclStatKey::from_chain_and_action(
            chain_type,
            AclStatType::Total,
        ));
        processor.increment_stat(AclStatKey::PacketsTotal);
    }

    fn classify_chain_type(
        is_in: bool,
        packet_info: &PacketInfo,
        my_ipv4: Option<Ipv4Addr>,
        is_local_ipv6: impl Fn(Ipv6Addr) -> bool,
    ) -> ChainType {
        if !is_in {
            return ChainType::Outbound;
        }

        let is_local_dst = packet_info.dst_ip == my_ipv4.unwrap_or(Ipv4Addr::UNSPECIFIED)
            || matches!(packet_info.dst_ip, IpAddr::V6(dst) if is_local_ipv6(dst));

        if is_local_dst {
            ChainType::Inbound
        } else {
            ChainType::Forward
        }
    }

    /// Common ACL processing logic
    pub fn process_packet_with_acl(
        &self,
        packet: &ZCPacket,
        is_in: bool,
        my_ipv4: Option<Ipv4Addr>,
        is_local_ipv6: impl Fn(Ipv6Addr) -> bool,
        route: &(dyn crate::peers::route::Route + Send + Sync + 'static),
    ) -> bool {
        if !self.acl_enabled.load(Ordering::Relaxed) {
            return true;
        }

        if packet.peer_manager_header().unwrap().packet_type != PacketType::Data as u8 {
            return true;
        }

        // Extract packet information
        let packet_info = match self.extract_packet_info(packet, route) {
            Some(info) => info,
            None => {
                tracing::warn!(
                    "Failed to extract packet info from {:?} packet, header: {:?}",
                    if is_in { "inbound" } else { "outbound" },
                    packet.peer_manager_header()
                );
                // allow all unknown packets
                return true;
            }
        };

        let chain_type = Self::classify_chain_type(is_in, &packet_info, my_ipv4, is_local_ipv6);

        // Get current processor atomically
        let processor = self.get_processor();

        // Process through ACL rules
        let acl_result = processor.process_packet(&packet_info, chain_type);

        self.handle_acl_result(&acl_result, &packet_info, chain_type, &processor);

        // Check if packet should be allowed
        match acl_result.action {
            Action::Allow | Action::Noop => {
                if matches!(chain_type, ChainType::Outbound) {
                    self.outbound_allow_records.insert(
                        OutboundAllowRecord::new_from_outbound_packet(&packet_info),
                        Instant::now(),
                    );
                }
                true
            }
            Action::Drop => {
                if is_in {
                    let record = OutboundAllowRecord::new_from_inbound_packet(&packet_info);
                    let entry = self.outbound_allow_records.entry(record);
                    if let dashmap::Entry::Occupied(mut entry) = entry {
                        entry.insert(Instant::now());
                        tracing::trace!(
                            "ACL: Allowing {:?} packet from {} to {} because of existing allow record, chain_type: {:?}",
                            packet_info.protocol,
                            packet_info.src_ip,
                            packet_info.dst_ip,
                            chain_type,
                        );
                        return true;
                    }
                }

                tracing::trace!(
                    "ACL: Dropping {:?} packet from {} to {}, chain_type: {:?}",
                    packet_info.protocol,
                    packet_info.src_ip,
                    packet_info.dst_ip,
                    chain_type,
                );

                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeSet, HashMap},
        net::{IpAddr, Ipv4Addr, Ipv6Addr},
        sync::Arc,
    };

    use async_trait::async_trait;
    use cidr::{Ipv4Cidr, Ipv6Cidr};
    use quanta::Instant;

    use easytier_proto::acl::{
        Acl, AclV1, Action, Chain, ChainType, GroupIdentity, GroupInfo, Protocol, Rule,
    };

    use crate::{
        config::PeerId,
        packet::{PacketType, ZCPacket},
        peers::acl::processor::PacketInfo,
        peers::route::{Route, RouteInterfaceBox},
        proto::core_peer::peer::Route as CoreRoute,
        proto::peer_rpc::RoutePeerInfo,
    };

    use super::{
        AclFilter, IP_PROTO_ICMP, IP_PROTO_TCP, IP_PROTO_UDP, OutboundAllowRecord, acl_protocol,
        parse_ip_packet, parse_transport_ports,
    };

    impl AclFilter {
        pub(crate) fn cleanup_task_is_stopped(&self) -> bool {
            self.clean_task.lock().unwrap().is_none()
        }
    }

    fn packet_info(dst_ip: IpAddr) -> PacketInfo {
        PacketInfo {
            src_ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            dst_ip,
            src_port: Some(1234),
            dst_port: Some(80),
            protocol: Protocol::Tcp,
            packet_size: 64,
            src_groups: Arc::new(Vec::new()),
            dst_groups: Arc::new(Vec::new()),
        }
    }

    /// 出站 ACL 测试桩：按 peer id 与按目标 IP 均可解析组。
    struct StubRoute {
        peer_groups: HashMap<u32, Vec<String>>,
        ip_peer: HashMap<IpAddr, u32>,
    }

    #[async_trait]
    impl Route for StubRoute {
        async fn open(&self, _interface: RouteInterfaceBox) -> Result<u8, ()> {
            Ok(0)
        }
        async fn close(&self) {}
        async fn get_next_hop(&self, _peer_id: PeerId) -> Option<PeerId> {
            None
        }
        async fn list_routes(&self) -> Vec<CoreRoute> {
            vec![]
        }
        async fn list_proxy_cidrs(&self) -> BTreeSet<Ipv4Cidr> {
            BTreeSet::new()
        }
        async fn list_proxy_cidrs_v6(&self) -> BTreeSet<Ipv6Cidr> {
            BTreeSet::new()
        }
        async fn get_peer_info(&self, _peer_id: PeerId) -> Option<RoutePeerInfo> {
            None
        }
        async fn get_peer_info_last_update_time(&self) -> Instant {
            Instant::now()
        }
        fn get_peer_groups(&self, peer_id: PeerId) -> Arc<Vec<String>> {
            Arc::new(self.peer_groups.get(&peer_id).cloned().unwrap_or_default())
        }
        fn get_peer_groups_by_ip_sync(&self, ip: &IpAddr) -> Arc<Vec<String>> {
            self.ip_peer
                .get(ip)
                .map(|peer_id| self.get_peer_groups(*peer_id))
                .unwrap_or_default()
        }
    }

    fn office_acl() -> Acl {
        Acl {
            acl_v1: Some(AclV1 {
                chains: vec![Chain {
                    name: "anf-acl".to_string(),
                    chain_type: ChainType::Outbound as i32,
                    description: String::new(),
                    enabled: true,
                    rules: vec![Rule {
                        name: "allow-office".to_string(),
                        description: String::new(),
                        priority: 100,
                        enabled: true,
                        protocol: Protocol::Any as i32,
                        ports: vec![],
                        source_ips: vec![],
                        destination_ips: vec![],
                        source_ports: vec![],
                        action: Action::Allow as i32,
                        rate_limit: 0,
                        burst_limit: 0,
                        stateful: false,
                        source_groups: vec!["office".to_string()],
                        destination_groups: vec!["office".to_string()],
                    }],
                    default_action: Action::Drop as i32,
                }],
                group: Some(GroupInfo {
                    declares: vec![GroupIdentity {
                        group_name: "office".to_string(),
                        group_secret: String::new(),
                    }],
                    members: vec!["office".to_string()],
                }),
            }),
        }
    }

    fn outbound_data_packet() -> ZCPacket {
        let mut ip = vec![0u8; 40];
        ip[0] = 0x45;
        ip[2..4].copy_from_slice(&40u16.to_be_bytes());
        ip[9] = IP_PROTO_TCP;
        ip[12..16].copy_from_slice(&[10, 0, 0, 1]);
        ip[16..20].copy_from_slice(&[10, 0, 0, 2]);
        ip[20..22].copy_from_slice(&1234u16.to_be_bytes());
        ip[22..24].copy_from_slice(&80u16.to_be_bytes());
        let mut packet = ZCPacket::new_with_payload(&ip);
        // 出站报文在 ACL 检查阶段目的 peer 尚未路由（to_peer_id=0）
        packet.fill_peer_manager_hdr(7, 0, PacketType::Data as u8);
        packet
    }

    #[test]
    fn parse_ipv4_tcp_packet_extracts_addrs_and_ports() {
        let mut packet = vec![0u8; 40];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&40u16.to_be_bytes());
        packet[9] = IP_PROTO_TCP;
        packet[12..16].copy_from_slice(&[10, 0, 0, 1]);
        packet[16..20].copy_from_slice(&[10, 0, 0, 2]);
        packet[20..22].copy_from_slice(&1234u16.to_be_bytes());
        packet[22..24].copy_from_slice(&80u16.to_be_bytes());

        let parsed = parse_ip_packet(&packet).unwrap();
        let (src_port, dst_port) =
            parse_transport_ports(parsed.protocol, parsed.transport_payload).unwrap();

        assert_eq!(parsed.src_ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
        assert_eq!(parsed.dst_ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)));
        assert_eq!(acl_protocol(parsed.protocol), Protocol::Tcp);
        assert_eq!(src_port, Some(1234));
        assert_eq!(dst_port, Some(80));
    }

    #[test]
    fn parse_ipv6_udp_packet_extracts_addrs_and_ports() {
        let src: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let dst: Ipv6Addr = "2001:db8::2".parse().unwrap();
        let mut packet = vec![0u8; 48];
        packet[0] = 0x60;
        packet[4..6].copy_from_slice(&8u16.to_be_bytes());
        packet[6] = IP_PROTO_UDP;
        packet[8..24].copy_from_slice(&src.octets());
        packet[24..40].copy_from_slice(&dst.octets());
        packet[40..42].copy_from_slice(&5353u16.to_be_bytes());
        packet[42..44].copy_from_slice(&53u16.to_be_bytes());

        let parsed = parse_ip_packet(&packet).unwrap();
        let (src_port, dst_port) =
            parse_transport_ports(parsed.protocol, parsed.transport_payload).unwrap();

        assert_eq!(parsed.src_ip, IpAddr::V6(src));
        assert_eq!(parsed.dst_ip, IpAddr::V6(dst));
        assert_eq!(acl_protocol(parsed.protocol), Protocol::Udp);
        assert_eq!(src_port, Some(5353));
        assert_eq!(dst_port, Some(53));
    }

    #[test]
    fn parse_ipv4_uses_declared_total_length() {
        let mut packet = vec![0u8; 40];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&24u16.to_be_bytes());
        packet[9] = IP_PROTO_TCP;
        packet[12..16].copy_from_slice(&[10, 0, 0, 1]);
        packet[16..20].copy_from_slice(&[10, 0, 0, 2]);
        packet[20..22].copy_from_slice(&1234u16.to_be_bytes());
        packet[22..24].copy_from_slice(&80u16.to_be_bytes());

        let parsed = parse_ip_packet(&packet).unwrap();

        assert_eq!(parsed.transport_payload.len(), 4);
        assert!(parse_transport_ports(parsed.protocol, parsed.transport_payload).is_none());
    }

    #[test]
    fn parse_ipv6_uses_declared_payload_length() {
        let mut packet = vec![0u8; 48];
        packet[0] = 0x60;
        packet[4..6].copy_from_slice(&4u16.to_be_bytes());
        packet[6] = IP_PROTO_UDP;
        packet[40..42].copy_from_slice(&5353u16.to_be_bytes());
        packet[42..44].copy_from_slice(&53u16.to_be_bytes());

        let parsed = parse_ip_packet(&packet).unwrap();

        assert_eq!(parsed.transport_payload.len(), 4);
        assert!(parse_transport_ports(parsed.protocol, parsed.transport_payload).is_none());
    }

    #[test]
    fn parse_ipv4_keeps_pnet_ihl_less_than_five_behavior() {
        let mut packet = vec![0u8; 40];
        packet[0] = 0x44;
        packet[2..4].copy_from_slice(&40u16.to_be_bytes());
        packet[9] = IP_PROTO_TCP;
        packet[20..22].copy_from_slice(&1234u16.to_be_bytes());
        packet[22..24].copy_from_slice(&80u16.to_be_bytes());

        let parsed = parse_ip_packet(&packet).unwrap();
        let (src_port, dst_port) =
            parse_transport_ports(parsed.protocol, parsed.transport_payload).unwrap();

        assert_eq!(parsed.transport_payload.len(), 20);
        assert_eq!(src_port, Some(1234));
        assert_eq!(dst_port, Some(80));
    }

    #[test]
    fn parse_ipv4_keeps_pnet_truncated_options_behavior() {
        let mut packet = vec![0u8; 20];
        packet[0] = 0x4f;
        packet[2..4].copy_from_slice(&60u16.to_be_bytes());
        packet[9] = IP_PROTO_ICMP;
        packet[12..16].copy_from_slice(&[10, 0, 0, 1]);
        packet[16..20].copy_from_slice(&[10, 0, 0, 2]);

        let parsed = parse_ip_packet(&packet).unwrap();
        let (src_port, dst_port) =
            parse_transport_ports(parsed.protocol, parsed.transport_payload).unwrap();

        assert_eq!(parsed.src_ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
        assert_eq!(parsed.dst_ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)));
        assert_eq!(acl_protocol(parsed.protocol), Protocol::Icmp);
        assert!(parsed.transport_payload.is_empty());
        assert_eq!(src_port, None);
        assert_eq!(dst_port, None);
    }

    #[test]
    fn classify_chain_type_treats_public_ipv6_lease_as_inbound() {
        let leased_ipv6 = Ipv6Addr::new(0x2001, 0xdb8, 0x100, 0, 0, 0, 0, 0x123);
        let packet_info = packet_info(IpAddr::V6(leased_ipv6));

        let chain =
            AclFilter::classify_chain_type(true, &packet_info, None, |ip| ip == leased_ipv6);

        assert_eq!(chain, ChainType::Inbound);
    }

    #[test]
    fn classify_chain_type_keeps_non_local_ipv6_as_forward() {
        let leased_ipv6 = Ipv6Addr::new(0x2001, 0xdb8, 0x100, 0, 0, 0, 0, 0x123);
        let packet_info = packet_info(IpAddr::V6(Ipv6Addr::new(
            0x2001, 0xdb8, 0xffff, 2, 0, 0, 0, 0x100,
        )));

        let chain =
            AclFilter::classify_chain_type(true, &packet_info, None, |ip| ip == leased_ipv6);

        assert_eq!(chain, ChainType::Forward);
    }

    #[tokio::test]
    async fn reload_rules_clears_outbound_allow_records() {
        let filter = AclFilter::new();
        filter.outbound_allow_records.insert(
            OutboundAllowRecord {
                src_ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
                dst_ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
                src_port: Some(1234),
                dst_port: Some(80),
                protocol: Protocol::Tcp,
            },
            Instant::now(),
        );
        assert_eq!(filter.outbound_allow_records.len(), 1);

        filter.reload_rules(Some(&Acl::default()));

        assert_eq!(filter.outbound_allow_records.len(), 0);

        filter.outbound_allow_records.insert(
            OutboundAllowRecord {
                src_ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
                dst_ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
                src_port: Some(4321),
                dst_port: Some(443),
                protocol: Protocol::Tcp,
            },
            Instant::now(),
        );
        assert_eq!(filter.outbound_allow_records.len(), 1);

        filter.reload_rules(None);

        assert_eq!(filter.outbound_allow_records.len(), 0);
    }

    #[tokio::test]
    async fn outbound_group_rule_matches_dst_groups_by_ip_fallback() {
        let filter = AclFilter::new();
        filter.reload_rules(Some(&office_acl()));

        let route = StubRoute {
            peer_groups: HashMap::from([
                (7u32, vec!["office".to_string()]),
                (8u32, vec!["office".to_string()]),
            ]),
            ip_peer: HashMap::from([(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)), 8u32)]),
        };

        let allowed =
            filter.process_packet_with_acl(&outbound_data_packet(), false, None, |_| false, &route);
        assert!(
            allowed,
            "目标 peer 未知时按 IP 回退解析组，office->office 应放行"
        );
    }

    #[tokio::test]
    async fn outbound_group_rule_drops_when_dst_groups_unknown() {
        let filter = AclFilter::new();
        filter.reload_rules(Some(&office_acl()));

        // 目标 IP 无对端（组未知）→ 规则不匹配 → 默认拒绝
        let route = StubRoute {
            peer_groups: HashMap::from([(7u32, vec!["office".to_string()])]),
            ip_peer: HashMap::new(),
        };

        let allowed =
            filter.process_packet_with_acl(&outbound_data_packet(), false, None, |_| false, &route);
        assert!(!allowed, "目标组未知时应默认拒绝");
    }
}
