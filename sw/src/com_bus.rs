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
