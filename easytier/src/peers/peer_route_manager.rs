use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use anyhow::Context;
use tokio::sync::Mutex;

use crate::{
    common::{
        ifcfg::{IfConfiger, IfConfiguerTrait},
        PeerId,
        error::Error,
    },
    proto::common::TunnelInfo,
};

/// Get default gateway for system
#[cfg(target_os = "linux")]
async fn get_default_gateway() -> Result<Option<IpAddr>, Error> {
    use std::fs;

    // Read /proc/net/route to find default gateway
    let content = fs::read_to_string("/proc/net/route")
        .map_err(|e| Error::ShellCommandError(format!("Failed to read /proc/net/route: {}", e)))?;

    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }

        // Destination field (index 1) - 00000000 means default route
        if parts[1] == "00000000" {
            // Gateway field (index 2) is in hex, need to convert
            let gateway_hex = parts[2];
            if let Ok(gateway_u32) = u32::from_str_radix(gateway_hex, 16) {
                let gateway = Ipv4Addr::from(gateway_u32.to_le_bytes());
                return Ok(Some(IpAddr::V4(gateway)));
            }
        }
    }

    Ok(None)
}

/// Get default gateway for system
#[cfg(target_os = "windows")]
async fn get_default_gateway() -> Result<Option<IpAddr>, Error> {
    use tokio::process::Command;

    let output = Command::new("route")
        .args(["print", "0.0.0.0"])
        .output()
        .await
        .map_err(|e| Error::ShellCommandError(format!("Failed to run route print: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.contains("0.0.0.0") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                if let Ok(gateway) = parts[2].parse::<Ipv4Addr>() {
                    return Ok(Some(IpAddr::V4(gateway)));
                }
            }
        }
    }

    Ok(None)
}

/// Get default gateway for system
#[cfg(any(target_os = "macos", target_os = "freebsd"))]
async fn get_default_gateway() -> Result<Option<IpAddr>, Error> {
    use tokio::process::Command;

    let output = Command::new("netstat")
        .args(["-nr"])
        .output()
        .await
        .map_err(|e| Error::ShellCommandError(format!("Failed to run netstat: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("default") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(gateway) = parts[1].parse::<IpAddr>() {
                    return Ok(Some(gateway));
                }
            }
        }
    }

    Ok(None)
}

/// Get interface name for a given IP address
async fn get_interface_for_ip(target_ip: IpAddr) -> Result<Option<String>, Error> {
    let interfaces = pnet::datalink::interfaces();

    for iface in interfaces {
        for ip_network in &iface.ips {
            if ip_network.ip() == target_ip {
                return Ok(Some(iface.name.clone()));
            }
        }
    }

    Ok(None)
}

/// Get interface name for a given destination IP
async fn get_interface_for_destination(destination: IpAddr) -> Result<Option<String>, Error> {
    use tokio::net::UdpSocket;

    let bind_addr = if destination.is_ipv4() {
        IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
    } else {
        IpAddr::V6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0))
    };

    let socket = UdpSocket::bind((bind_addr, 0)).await
        .map_err(|e| Error::ShellCommandError(format!("Failed to bind socket: {}", e)))?;

    socket.connect((destination, 80)).await
        .map_err(|e| Error::ShellCommandError(format!("Failed to connect socket: {}", e)))?;

    let local_addr = socket.local_addr()
        .map_err(|e| Error::ShellCommandError(format!("Failed to get local addr: {}", e)))?;

    get_interface_for_ip(local_addr.ip()).await
}

/// Peer route manager
/// Automatically manages system routes for peer connections
pub struct PeerRouteManager {
    routes: Arc<Mutex<Vec<PeerRoute>>>,
    ifconfiger: IfConfiger,
}

#[derive(Debug, Clone)]
pub struct PeerRoute {
    peer_id: PeerId,
    destination: IpAddr,
    interface: String,
    prefix: u8,
}

impl PeerRouteManager {
    pub fn new() -> Self {
        Self {
            routes: Arc::new(Mutex::new(Vec::new())),
            ifconfiger: IfConfiger {},
        }
    }
}

impl Default for PeerRouteManager {
    fn default() -> Self {
        Self::new()
    }
}

    /// Add a route for a peer connection
    pub async fn add_peer_route(
        &self,
        peer_id: PeerId,
        tunnel_info: &TunnelInfo,
    ) -> Result<(), Error> {
        let remote_addr_url = tunnel_info.remote_addr.as_ref()
            .ok_or_else(|| Error::ShellCommandError("Remote addr not found in tunnel info".to_string()))?;

        let remote_addr: url::Url = remote_addr_url.clone().into();
        let remote_socket_addr = remote_addr.socket_addrs(|| Some(1))
            .map_err(|e| Error::ShellCommandError(format!("Failed to parse remote addr: {}", e)))?;

        if remote_socket_addr.is_empty() {
            return Err(Error::ShellCommandError("No socket addresses found".to_string()));
        }

        let remote_ip = remote_socket_addr[0].ip();

        // Get default gateway
        let gateway = get_default_gateway().await
            .map_err(|e| Error::ShellCommandError(format!("Failed to get default gateway: {}", e)))?
            .ok_or_else(|| Error::ShellCommandError("No default gateway found".to_string()))?;

        // Get interface name for this destination
        let interface_name = get_interface_for_destination(remote_ip).await
            .map_err(|e| Error::ShellCommandError(format!("Failed to get interface: {}", e)))?
            .ok_or_else(|| Error::ShellCommandError("No interface found for destination".to_string()))?;

        // Add route to system
        let prefix = 32; // /32 for single host route
        let metric = Some(100); // Set a metric value

        match remote_ip {
            IpAddr::V4(ip) => {
                tracing::info!(
                    "Adding IPv4 route for peer {}: dest={}, gateway={}, interface={}, metric={:?}",
                    peer_id, ip, gateway, interface_name, metric
                );

                self.ifconfiger.add_ipv4_route(&interface_name, ip, prefix, metric).await
                    .context("Failed to add IPv4 route")?;
            }
            IpAddr::V6(ip) => {
                tracing::info!(
                    "Adding IPv6 route for peer {}: dest={}, gateway={}, interface={}, metric={:?}",
                    peer_id, ip, gateway, interface_name, metric
                );

                self.ifconfiger.add_ipv6_route(&interface_name, ip, prefix, metric).await
                    .context("Failed to add IPv6 route")?;
            }
        }

        // Store route information
        let route = PeerRoute {
            peer_id,
            destination: remote_ip,
            interface: interface_name.clone(),
            prefix,
        };

        self.routes.lock().await.push(route);

        tracing::info!(
            "Successfully added route for peer {} to {} via {}",
            peer_id, remote_ip, interface_name
        );

        Ok(())
    }

    /// Remove all routes for a specific peer
    pub async fn remove_peer_routes(&self, peer_id: PeerId) -> Result<(), Error> {
        let mut routes = self.routes.lock().await;
        let mut to_remove = Vec::new();

        // Find all routes for this peer
        routes.retain(|route| {
            if route.peer_id == peer_id {
                to_remove.push(route.clone());
                false
            } else {
                true
            }
        });

        // Remove routes from system
        for route in to_remove {
            tracing::info!(
                "Removing route for peer {}: dest={}, interface={}",
                peer_id, route.destination, route.interface
            );

            let result = match route.destination {
                IpAddr::V4(ip) => {
                    self.ifconfiger.remove_ipv4_route(&route.interface, ip, route.prefix).await
                }
                IpAddr::V6(ip) => {
                    self.ifconfiger.remove_ipv6_route(&route.interface, ip, route.prefix).await
                }
            };

            if let Err(e) = result {
                tracing::warn!(
                    "Failed to remove route for peer {} to {}: {:?}",
                    peer_id, route.destination, e
                );
            }
        }

        Ok(())
    }

    /// List all managed routes
    pub async fn list_routes(&self) -> Vec<PeerRoute> {
        self.routes.lock().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn test_get_default_gateway() {
        let gateway = get_default_gateway().await;
        println!("Default gateway: {:?}", gateway);
    }

    #[tokio::test]
    async fn test_get_interface_for_destination() {
        let result = get_interface_for_destination("8.8.8.8".parse().unwrap()).await;
        println!("Interface for 8.8.8.8: {:?}", result);
    }

    #[tokio::test]
    async fn test_peer_route_manager() {
        let manager = PeerRouteManager::new();
        let routes = manager.list_routes().await;
        assert!(routes.is_empty());
    }
}
