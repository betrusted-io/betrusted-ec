/// Incoming Ethernet frames get sorted into these bins by the packet filter
#[derive(Copy, Clone, PartialEq)]
pub enum FilterBin {
    DropNoise, // Malformed packet
    DropEType, // Unsupported Ethernet protocol (perhaps IPv6, 802.1Q VLAN, etc)
    DropDhcp, // DHCP noise (perhaps malformed options or server response out of sync with client state machine)
    DropMulti, // Multicast (likely mDNS)
    DropProto, // Unsupported IP layer protocol
    DropFrag, // Unsupported packet fragment
    DropIpCk, // Bad IP header checksum
    DropUdpCk, // Bad UDP header checksum
    Arp,
    Icmp,
    Dhcp,
    Udp,
    ComFwd, // Forward to COM net bridge
}

/// FilterStats maintains diagnostic statistics on how inbound packets were filtered
pub struct FilterStats {
    pub drop_noise: u16,
    pub drop_etype: u16,
    pub drop_dhcp: u16,
    pub drop_multi: u16,
    pub drop_proto: u16,
    pub drop_frag: u16,
    pub drop_ipck: u16,
    pub drop_udpck: u16,
    pub arp: u16,
    pub icmp: u16,
    pub dhcp: u16,
    pub udp: u16,
    pub com_fwd: u16,
}
impl FilterStats {
    /// Initialize a new filter stats struct
    pub const fn new_all_zero() -> FilterStats {
        FilterStats {
            drop_noise: 0,
            drop_etype: 0,
            drop_dhcp: 0,
            drop_multi: 0,
            drop_proto: 0,
            drop_frag: 0,
            drop_ipck: 0,
            drop_udpck: 0,
            arp: 0,
            icmp: 0,
            dhcp: 0,
            udp: 0,
            com_fwd: 0,
        }
    }

    /// Zero all the counters
    pub fn reset(&mut self) {
        *self = Self::new_all_zero();
    }

    /// Increment the counter for the specified filter bin
    pub fn inc_count_for(&mut self, filter_bin: FilterBin) {
        match filter_bin {
            FilterBin::DropNoise => self.drop_noise = self.drop_noise.saturating_add(1),
            FilterBin::DropEType => self.drop_etype = self.drop_etype.saturating_add(1),
            FilterBin::DropDhcp => self.drop_dhcp = self.drop_dhcp.saturating_add(1),
            FilterBin::DropMulti => self.drop_multi = self.drop_multi.saturating_add(1),
            FilterBin::DropProto => self.drop_proto = self.drop_proto.saturating_add(1),
            FilterBin::DropFrag => self.drop_frag = self.drop_frag.saturating_add(1),
            FilterBin::DropIpCk => self.drop_ipck = self.drop_ipck.saturating_add(1),
            FilterBin::DropUdpCk => self.drop_udpck = self.drop_udpck.saturating_add(1),
            FilterBin::Arp => self.arp = self.arp.saturating_add(1),
            FilterBin::Icmp => self.icmp = self.icmp.saturating_add(1),
            FilterBin::Dhcp => self.dhcp = self.dhcp.saturating_add(1),
            FilterBin::Udp => self.udp = self.udp.saturating_add(1),
            FilterBin::ComFwd => self.com_fwd = self.com_fwd.saturating_add(1),
        };
    }
}
