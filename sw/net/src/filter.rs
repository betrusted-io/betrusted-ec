/// Incoming Ethernet frames get sorted into these bins by the packet filter
#[derive(Copy, Clone)]
pub enum FilterBin {
    DropNoise, // Malformed packet
    DropEType, // Unsupported Ethernet protocol (perhaps IPv6, 802.1Q VLAN, etc)
    DropMulti, // Multicast (likely mDNS)
    DropProto, // Unsupported IP layer protocol (perhaps TCP)
    DropFrag,  // Unsupported packet fragment
    ArpReq,
    ArpReply,
    Icmp,
    Dhcp,
    Udp,
}

/// FilterStats maintains diagnostic statistics on how inbound packets were filtered
pub struct FilterStats {
    pub drop_noise: u16,
    pub drop_etype: u16,
    pub drop_multi: u16,
    pub drop_proto: u16,
    pub drop_frag: u16,
    pub arp_req: u16,
    pub arp_reply: u16,
    pub icmp: u16,
    pub dhcp: u16,
    pub udp: u16,
}
impl FilterStats {
    /// Initialize a new filter stats struct
    pub const fn new_all_zero() -> FilterStats {
        FilterStats {
            drop_noise: 0,
            drop_etype: 0,
            drop_multi: 0,
            drop_proto: 0,
            drop_frag: 0,
            arp_req: 0,
            arp_reply: 0,
            icmp: 0,
            dhcp: 0,
            udp: 0,
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
            FilterBin::DropMulti => self.drop_multi = self.drop_multi.saturating_add(1),
            FilterBin::DropProto => self.drop_proto = self.drop_proto.saturating_add(1),
            FilterBin::DropFrag => self.drop_frag = self.drop_frag.saturating_add(1),
            FilterBin::ArpReq => self.arp_req = self.arp_req.saturating_add(1),
            FilterBin::ArpReply => self.arp_reply = self.arp_reply.saturating_add(1),
            FilterBin::Icmp => self.icmp = self.icmp.saturating_add(1),
            FilterBin::Dhcp => self.dhcp = self.dhcp.saturating_add(1),
            FilterBin::Udp => self.udp = self.udp.saturating_add(1),
        };
    }
}
