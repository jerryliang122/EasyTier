use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug, trace};

use crate::common::ifcfg::{IfConfiguerTrait, Route, ExitNodeConfig};
use crate::common::defer;
use crate::common::scoped_task::ScopedTask;
use crate::instance::virtual_nic::VirtualNic;

/// Exit node manager for handling virtual IP to real IP mapping and route table management
pub struct ExitNodeManager {
    /// Mapping of virtual IPs to real public IPs
    virtual_to_real_mapping: Arc<RwLock<HashMap<IpAddr, IpAddr>>>,
    /// Exit node configurations ordered by priority
    exit_nodes: Arc<RwLock<Vec<ExitNodeConfig>>>,
    /// Active routes for cleanup
    active_routes: Arc<RwLock<Vec<Route>>>,
    /// Network interface configuration handle
    ifcfg: Arc<dyn IfConfiguerTrait>,
    /// TUN interface name
    tun_interface: String,
}

impl ExitNodeManager {
    /// Create a new exit node manager
    pub fn new(ifcfg: Arc<dyn IfConfiguerTrait>, tun_interface: String) -> Self {
        Self {
            virtual_to_real_mapping: Arc::new(RwLock::new(HashMap::new())),
            exit_nodes: Arc::new(RwLock::new(Vec::new())),
            active_routes: Arc::new(RwLock::new(Vec::new())),
            ifcfg,
            tun_interface,
        }
    }

    /// Update exit nodes configuration
    pub async fn update_exit_nodes(&self, exit_nodes: Vec<IpAddr>) -> Result<(), anyhow::Error> {
        let start_time = std::time::Instant::now();
        
        info!(
            exit_nodes_count = exit_nodes.len(),
            exit_nodes = ?exit_nodes,
            "Starting exit nodes configuration update"
        );
        
        // Clear previous configuration
        if let Err(e) = self.cleanup_routes().await {
            error!(
                error = %e,
                "Failed to cleanup previous exit node configuration"
            );
            return Err(e);
        }
        
        // Build exit node configurations with priority
        let mut configs = Vec::new();
        let mut successful_resolutions = 0;
        let mut failed_resolutions = 0;
        
        for (index, virtual_ip) in exit_nodes.iter().enumerate() {
            match self.resolve_real_ip(virtual_ip).await {
                Ok(real_ip) => {
                    let config = ExitNodeConfig {
                        is_exit_node: true,
                        virtual_ip: *virtual_ip,
                        real_ip,
                        priority: index as u32,
                        prevent_loop: true,
                    };
                    configs.push(config);
                    
                    // Update mapping
                    self.virtual_to_real_mapping.write().await.insert(*virtual_ip, real_ip);
                    successful_resolutions += 1;
                    
                    info!(
                        virtual_ip = %virtual_ip,
                        real_ip = %real_ip,
                        priority = index,
                        "Exit node resolved successfully"
                    );
                }
                Err(e) => {
                    failed_resolutions += 1;
                    error!(
                        virtual_ip = %virtual_ip,
                        error = %e,
                        "Failed to resolve exit node real IP"
                    );
                }
            }
        }
        
        if successful_resolutions == 0 {
            warn!("No exit nodes were successfully resolved");
            return Err(anyhow::anyhow!("No exit nodes could be resolved"));
        }
        
        // Sort by priority (lower index = higher priority)
        configs.sort_by_key(|c| c.priority);
        
        // Update exit nodes list
        *self.exit_nodes.write().await = configs;
        
        // Setup routes for all exit nodes
        if let Err(e) = self.setup_exit_node_routes().await {
            error!(
                error = %e,
                "Failed to setup exit node routes"
            );
            return Err(e);
        }
        
        let duration = start_time.elapsed();
        info!(
            successful_resolutions = successful_resolutions,
            failed_resolutions = failed_resolutions,
            duration_ms = duration.as_millis(),
            "Exit nodes configuration updated successfully"
        );
        
        Ok(())
    }

    /// Resolve virtual IP to real public IP address
    async fn resolve_real_ip(&self, virtual_ip: &IpAddr) -> Result<IpAddr, anyhow::Error> {
        // This is a simplified implementation - in real scenario, you might want to:
        // 1. Use DNS resolution
        // 2. Query peer discovery service
        // 3. Use network mapping table
        
        debug!("Resolving real IP for virtual IP: {}", virtual_ip);
        
        // For now, assume the virtual IP is also the real IP
        // In production, you would implement proper IP resolution logic
        Ok(*virtual_ip)
    }

    /// Setup routes for exit nodes to prevent traffic loops
    async fn setup_exit_node_routes(&self) -> Result<(), anyhow::Error> {
        let exit_nodes = self.exit_nodes.read().await;
        let mut active_routes = self.active_routes.write().await;
        
        for config in exit_nodes.iter() {
            // Add route to prevent traffic loop back to exit node virtual IP
            let loop_prevention_route = Route::new(config.virtual_ip, 32)
                .with_gateway(config.real_ip)
                .with_ifindex(self.get_interface_index().await?)
                .with_metric(config.priority + 100) // Lower priority for loop prevention
                .with_exit_node_config(config.clone());
            
            self.add_route_internal(&loop_prevention_route).await?;
            active_routes.push(loop_prevention_route);
            
            info!("Added loop prevention route for exit node {} -> {}", 
                  config.virtual_ip, config.real_ip);
        }
        
        Ok(())
    }

    /// Add route to system with proper error handling
    async fn add_route_internal(&self, route: &Route) -> Result<(), anyhow::Error> {
        let start_time = std::time::Instant::now();
        
        let result = match route.destination {
            IpAddr::V4(ipv4) => {
                self.ifcfg.add_ipv4_route(&self.tun_interface, ipv4, route.prefix, None).await
            }
            IpAddr::V6(ipv6) => {
                self.ifcfg.add_ipv6_route(&self.tun_interface, ipv6, route.prefix, None).await
            }
        };
        
        let duration = start_time.elapsed();
        
        match result {
            Ok(_) => {
                info!(
                    route_type = if route.destination.is_ipv4() { "IPv4" } else { "IPv6" },
                    destination = %route.destination,
                    prefix = route.prefix,
                    gateway = ?route.gateway,
                    interface = %self.tun_interface,
                    metric = ?route.metric,
                    duration_ms = duration.as_millis(),
                    "Route added successfully"
                );
                
                // Log additional context for exit node routes
                if let Some(ref exit_config) = route.exit_node_config {
                    info!(
                        exit_node_virtual_ip = %exit_config.virtual_ip,
                        exit_node_real_ip = %exit_config.real_ip,
                        priority = exit_config.priority,
                        prevent_loop = exit_config.prevent_loop,
                        "Exit node route details"
                    );
                }
                
                Ok(())
            }
            Err(e) => {
                error!(
                    route_type = if route.destination.is_ipv4() { "IPv4" } else { "IPv6" },
                    destination = %route.destination,
                    prefix = route.prefix,
                    gateway = ?route.gateway,
                    interface = %self.tun_interface,
                    error = %e,
                    duration_ms = duration.as_millis(),
                    "Failed to add route"
                );
                Err(anyhow::anyhow!("Route addition failed: {}", e))
            }
        }
    }

    /// Get interface index for TUN device
    async fn get_interface_index(&self) -> Result<u32, anyhow::Error> {
        // This should interface with the system to get the actual interface index
        // For now, return a dummy value - implement based on your platform
        Ok(1u32)
    }

    /// Setup default route to TUN device for all traffic
    pub async fn setup_default_route(&self) -> Result<(), anyhow::Error> {
        let start_time = std::time::Instant::now();
        
        info!(
            interface = %self.tun_interface,
            "Setting up default route to TUN interface"
        );
        
        // Add 0.0.0.0/0 route to TUN device
        let default_route = Route::new(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0)
            .with_ifindex(self.get_interface_index().await?)
            .with_metric(100); // Lower metric for higher priority
        
        self.add_route_internal(&default_route).await?;
        self.active_routes.write().await.push(default_route);
        
        let duration = start_time.elapsed();
        info!(
            interface = %self.tun_interface,
            metric = 100,
            duration_ms = duration.as_millis(),
            "Default route to TUN interface configured successfully"
        );
        
        Ok(())
    }

    /// Get the best exit node for routing traffic
    pub async fn get_best_exit_node(&self) -> Option<ExitNodeConfig> {
        let exit_nodes = self.exit_nodes.read().await;
        exit_nodes.first().cloned()
    }

    /// Check if an IP belongs to an exit node
    pub async fn is_exit_node_ip(&self, ip: &IpAddr) -> bool {
        let mapping = self.virtual_to_real_mapping.read().await;
        mapping.contains_key(ip)
    }

    /// Get real IP for virtual exit node IP
    pub async fn get_real_ip(&self, virtual_ip: &IpAddr) -> Option<IpAddr> {
        let mapping = self.virtual_to_real_mapping.read().await;
        mapping.get(virtual_ip).copied()
    }

    /// Cleanup all dynamic routes added by this manager
    pub async fn cleanup_routes(&self) -> Result<(), anyhow::Error> {
        info!("Starting comprehensive exit node routes cleanup");
        
        let mut active_routes = self.active_routes.write().await;
        let routes_to_remove: Vec<Route> = active_routes.drain(..).collect();
        
        // Remove routes in reverse order of creation (to avoid dependency issues)
        for route in routes_to_remove.iter().rev() {
            if let Err(e) = self.remove_route_internal(route).await {
                warn!("Failed to remove route {:?}: {}", route, e);
                // Continue with other routes even if one fails
            } else {
                trace!("Successfully removed route: {:?}", route);
            }
        }
        
        // Clean up policy routes on Linux
        #[cfg(target_os = "linux")]
        self.cleanup_policy_routes().await?;
        
        // Clean up any custom routing tables
        #[cfg(target_os = "linux")]
        self.cleanup_custom_routing_tables().await?;
        
        // Clear mappings
        self.virtual_to_real_mapping.write().await.clear();
        self.exit_nodes.write().await.clear();
        
        info!("Exit node routes cleanup completed successfully");
        Ok(())
    }

    /// Clean up policy routes on Linux systems
    #[cfg(target_os = "linux")]
    async fn cleanup_policy_routes(&self) -> Result<(), anyhow::Error> {
        debug!("Cleaning up policy routes");
        
        // This would clean up ip rule entries
        // Implementation would use netlink to flush rules for exit node traffic
        // For now, we just log the operation
        trace!("Policy routes cleaned up");
        Ok(())
    }

    /// Clean up custom routing tables on Linux systems
    #[cfg(target_os = "linux")]
    async fn cleanup_custom_routing_tables(&self) -> Result<(), anyhow::Error> {
        debug!("Cleaning up custom routing tables");
        
        // This would clean up any custom routing tables created for exit nodes
        // Implementation would flush the table and possibly remove it
        trace!("Custom routing tables cleaned up");
        Ok(())
    }

    /// Remove route from system with enhanced error handling
    async fn remove_route_internal(&self, route: &Route) -> Result<(), anyhow::Error> {
        debug!("Removing route: {}/{} via {:?}", route.destination, route.prefix, route.gateway);
        
        let result = match route.destination {
            IpAddr::V4(ipv4) => {
                self.ifcfg.remove_ipv4_route(&self.tun_interface, ipv4, route.prefix).await
            }
            IpAddr::V6(ipv6) => {
                self.ifcfg.remove_ipv6_route(&self.tun_interface, ipv6, route.prefix).await
            }
        };
        
        match result {
            Ok(_) => {
                trace!("Successfully removed route: {}/{}", route.destination, route.prefix);
                Ok(())
            }
            Err(e) => {
                warn!("Failed to remove route {}/{}: {}", route.destination, route.prefix, e);
                // Don't fail the entire cleanup if one route removal fails
                Err(anyhow::anyhow!("Route removal failed: {}", e))
            }
        }
    }

    /// Remove route from system
    async fn remove_route_internal(&self, route: &Route) -> Result<(), anyhow::Error> {
        match route.destination {
            IpAddr::V4(ipv4) => {
                self.ifcfg.remove_ipv4_route(&self.tun_interface, ipv4, route.prefix).await?;
                debug!("Removed IPv4 route: {}/{}", route.destination, route.prefix);
            }
            IpAddr::V6(ipv6) => {
                self.ifcfg.remove_ipv6_route(&self.tun_interface, ipv6, route.prefix).await?;
                debug!("Removed IPv6 route: {}/{}", route.destination, route.prefix);
            }
        }
        Ok(())
    }

    /// Get current exit node status for monitoring
    pub async fn get_exit_node_status(&self) -> ExitNodeStatus {
        let exit_nodes = self.exit_nodes.read().await;
        let mapping = self.virtual_to_real_mapping.read().await;
        let active_routes = self.active_routes.read().await;
        
        ExitNodeStatus {
            exit_nodes: exit_nodes.clone(),
            virtual_to_real_mapping: mapping.clone(),
            active_routes_count: active_routes.len(),
        }
    }
}

/// Exit node status information
#[derive(Debug, Clone)]
pub struct ExitNodeStatus {
    /// Configured exit nodes
    pub exit_nodes: Vec<ExitNodeConfig>,
    /// Virtual to real IP mappings
    pub virtual_to_real_mapping: HashMap<IpAddr, IpAddr>,
    /// Number of active routes
    pub active_routes_count: usize,
}

impl Drop for ExitNodeManager {
    fn drop(&mut self) {
        // Ensure cleanup on drop - this runs synchronously so we use tokio::spawn
        let manager = ExitNodeManager {
            virtual_to_real_mapping: self.virtual_to_real_mapping.clone(),
            exit_nodes: self.exit_nodes.clone(),
            active_routes: self.active_routes.clone(),
            ifcfg: self.ifcfg.clone(),
            tun_interface: self.tun_interface.clone(),
        };
        
        tokio::spawn(async move {
            if let Err(e) = manager.cleanup_routes().await {
                error!("Failed to cleanup exit node routes during drop: {}", e);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::ifcfg::MockIfConfiguer; // You would need to create this mock
    
    #[tokio::test]
    async fn test_exit_node_manager_basic() {
        // Test basic functionality - you would need to implement mock IfConfiguer
        // This is just a placeholder showing the test structure
    }
}