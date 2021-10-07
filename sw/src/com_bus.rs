use betrusted_hal::hal_time::get_time_ms;
use utralib::generated::{utra, CSR, HW_COM_BASE};
use volatile::Volatile;

/*pub fn com_int_handler(_irq_no: usize) {
    let mut com_csr = CSR::new(HW_COM_BASE as *mut u32);
    // nop handler, here just to wake up the CPU in case of an incoming SPI packet and run the normal loop
    com_csr.wfo(utra::com::EV_PENDING_SPI_AVAIL, 1);
}*/

pub fn com_tx(tx: u16) {
    let com_ptr: *mut u32 = utralib::HW_COM_MEM as *mut u32;
    let com_fifo = com_ptr as *mut Volatile<u32>;

    unsafe {
        (*com_fifo).write(tx as u32);
    }
}

pub fn com_rx(timeout: u32) -> Result<u16, &'static str> {
    let com_csr = CSR::new(HW_COM_BASE as *mut u32);
    let com_rd_ptr: *mut u32 = utralib::HW_COM_MEM as *mut u32;
    let com_rd = com_rd_ptr as *mut Volatile<u32>;

    if timeout != 0 && (com_csr.rf(utra::com::STATUS_RX_AVAIL) == 0) {
        let start = get_time_ms();
        loop {
            if com_csr.rf(utra::com::STATUS_RX_AVAIL) == 1 {
                break;
            } else if start + timeout < get_time_ms() {
                return Err("timeout");
            }
        }
    }
    Ok(unsafe { (*com_rd).read() as u16 })
}

pub struct ComInterrupts {
    state: u16,
    rx_len: u16,
    mask: u16,
    saw_ack: bool,
}
#[allow(dead_code)]
impl ComInterrupts {
    pub fn new() -> Self {
        ComInterrupts {
            state: 0,
            rx_len: 0,
            mask: 0,
            saw_ack: false,
        }
    }
    /// getter for pin state logic
    pub fn update_irq_pin(&mut self) {
        let mut com_csr = CSR::new(utralib::HW_COM_BASE as *mut u32);
        if (self.state & self.mask) != 0 {
            if !self.saw_ack {
                com_csr.rmwf(utra::com::CONTROL_HOST_INT, 1);
            } else {
                // drop the IRQ line to create a new edge, in case we have a new interrupt despite the ack
                com_csr.rmwf(utra::com::CONTROL_HOST_INT, 0);
                self.saw_ack = false;
            }
        } else {
            com_csr.rmwf(utra::com::CONTROL_HOST_INT, 0);
            self.saw_ack = false;
        }
    }
    /// getter/setters from internal logic (wf200, etc.)
    pub fn set_rx_ready(&mut self, len: u16) {
        self.rx_len = len;
        self.state |= com_rs::INT_WLAN_RX_READY;
    }
    pub fn ack_rx_ready(&mut self) {
        self.rx_len = 0;
        self.state &= !com_rs::INT_WLAN_RX_READY;
        self.saw_ack = true;
    }
    pub fn set_ipconf_update(&mut self) {
        self.state |= com_rs::INT_WLAN_IPCONF_UPDATE;
    }
    pub fn ack_ipconf_update(&mut self) {
        self.state &= !com_rs::INT_WLAN_IPCONF_UPDATE;
        self.saw_ack = true;
    }
    pub fn set_ssid_update(&mut self) {
        self.state |= com_rs::INT_WLAN_SSID_UPDATE;
    }
    pub fn ack_ssid_update(&mut self) {
        self.state &= !com_rs::INT_WLAN_SSID_UPDATE;
        self.saw_ack = true;
    }
    pub fn set_battery_critical(&mut self) {
        self.state |= com_rs::INT_BATTERY_CRITICAL;
    }
    pub fn ack_battery_critical(&mut self) {
        self.state &= !com_rs::INT_BATTERY_CRITICAL;
        self.saw_ack = true;
    }

    /// getters/setters for COM bus interface
    pub fn get_mask(&self) -> u16 { self.mask }
    pub fn set_mask(&mut self, new_mask: u16) { self.mask = new_mask; }
    pub fn get_state(&self) -> (u16, u16) { (self.state, self.rx_len) }
    pub fn ack(&mut self, acks: u16) {
        self.state &= !acks;
        self.saw_ack = true;
    }
}