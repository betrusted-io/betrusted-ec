use betrusted_hal::hal_time::get_time_ms;
use com_rs::ConnectResult;
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
    rx_len_bytes: u16,
    mask: u16,
    retrigger: bool,
}
#[allow(dead_code)]
impl ComInterrupts {
    pub fn new() -> Self {
        ComInterrupts {
            state: 0,
            rx_len_bytes: 0,
            mask: 0,
            retrigger: false,
        }
    }
    /// getter for pin state logic
    pub fn update_irq_pin(&mut self) {
        let mut com_csr = CSR::new(utralib::HW_COM_BASE as *mut u32);
        if (self.state & self.mask) != 0 {
            if !self.retrigger {
                com_csr.rmwf(utra::com::CONTROL_HOST_INT, 1);
            } else {
                // drop the IRQ line to create a new edge, in case we have a new interrupt despite the ack
                com_csr.rmwf(utra::com::CONTROL_HOST_INT, 0);
                self.retrigger = false;
            }
        } else {
            com_csr.rmwf(utra::com::CONTROL_HOST_INT, 0);
            self.retrigger = false;
        }
    }
    /// getter/setters from internal logic (wf200, etc.)
    pub fn set_rx_ready(&mut self, len: u16) {
        // don't overwrite the connect result in case we got an Rx packet right after connecting
        if (self.state & com_rs::INT_WLAN_CONNECT_EVENT) == 0 {
            self.rx_len_bytes = len;
        }
        if self.state & com_rs::INT_WLAN_RX_READY != 0 {
            // if we're getting a second packet before the prior one was serviced, fake an ack
            // so that the interrupt edge fires again
            self.retrigger = true;
        } else {
            self.state |= com_rs::INT_WLAN_RX_READY;
        }
    }
    pub fn ack_rx_ready(&mut self) {
        // don't overwrite the connect result in case we had a delayed ack before we got the result read
        if (self.state & com_rs::INT_WLAN_CONNECT_EVENT) == 0 {
            self.rx_len_bytes = 0;
        }
        self.state &= !com_rs::INT_WLAN_RX_READY;
    }
    pub fn set_disconnect(&mut self) {
        if self.state & com_rs::INT_WLAN_DISCONNECT != 0 {
            // fake an ack so that the interrupt edge fires again
            self.retrigger = true;
        } else {
            self.state |= com_rs::INT_WLAN_DISCONNECT;
        }
    }
    pub fn ack_disconnect(&mut self) {
        self.state &= !com_rs::INT_WLAN_DISCONNECT;
    }
    pub fn set_connect_result(&mut self, result: ConnectResult) {
        if self.state & com_rs::INT_WLAN_CONNECT_EVENT != 0 {
            self.retrigger = true;
        } else {
            self.state |= com_rs::INT_WLAN_CONNECT_EVENT;
        }
        self.rx_len_bytes = result as u16;
    }
    pub fn ack_connect_result(&mut self) {
        self.state &= !com_rs::INT_WLAN_CONNECT_EVENT;
    }
    pub fn set_ipconf_update(&mut self) {
        self.state |= com_rs::INT_WLAN_IPCONF_UPDATE;
    }
    pub fn ack_ipconf_update(&mut self) {
        self.state &= !com_rs::INT_WLAN_IPCONF_UPDATE;
    }
    pub fn set_ssid_update(&mut self) {
        self.state |= com_rs::INT_WLAN_SSID_UPDATE;
    }
    pub fn ack_ssid_update(&mut self) {
        self.state &= !com_rs::INT_WLAN_SSID_UPDATE;
    }
    pub fn set_ssid_finished(&mut self) {
        self.state |= com_rs::INT_WLAN_SSID_FINISHED;
    }
    pub fn ack_ssid_finished(&mut self) {
        self.state &= !com_rs::INT_WLAN_SSID_FINISHED;
    }
    pub fn set_battery_critical(&mut self) {
        self.state |= com_rs::INT_BATTERY_CRITICAL;
    }
    pub fn ack_battery_critical(&mut self) {
        self.state &= !com_rs::INT_BATTERY_CRITICAL;
    }
    pub fn set_tx_error(&mut self) {
        self.state |= com_rs::INT_WLAN_TX_ERROR;
    }
    pub fn ack_tx_error(&mut self) {
        self.state &= !com_rs::INT_WLAN_TX_ERROR;
    }
    pub fn set_rx_error(&mut self) {
        self.state |= com_rs::INT_WLAN_RX_ERROR;
    }
    pub fn ack_rx_error(&mut self) {
        self.state &= !com_rs::INT_WLAN_RX_ERROR;
    }

    /// getters/setters for COM bus interface
    pub fn get_mask(&self) -> u16 { self.mask }
    pub fn set_mask(&mut self, new_mask: u16) {
        self.retrigger = true; // the intention is to cause any pre-existing interrupts to fire
        self.mask = new_mask;
    }
    pub fn get_state(&self) -> [u16; 2] { [self.state & self.mask, self.rx_len_bytes] }
    pub fn ack(&mut self, acks: u16) {
        self.state &= !acks;
    }
}