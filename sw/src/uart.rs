// use debug;
// use debug::{log, logln, sprint, sprintln, LL};
use utralib::generated::{utra, CSR};

// const LOG_LEVEL: LL = LL::Debug;

/// Debug UART input is protected by a state machine to detect wake sequence: A T Newline
/// The wake sequence state machine is designed to be case sensitive but tolerate
/// different OS-specific styles of line-ending sequences. For example, any of these
/// strings should work to wake from bypass mode: "AT\r\n", "AT\r", "AT\n".
#[derive(Copy, Clone, PartialEq)]
pub enum RxState {
    BypassOnAwaitA = 0,
    ExpectT,
    ExpectCROrLF,
    Waking,
    BypassOff,
}

/// Empty the UART RX buffer
pub fn drain_rx_buf() {
    let mut uart_csr = CSR::new(utra::uart::HW_UART_BASE as *mut u8);
    for _ in 0..32 {
        let no_pending_events = uart_csr.rf(utra::uart::EV_PENDING_RX) == 0;
        let rx_buffer_empty = uart_csr.rf(utra::uart::RXEMPTY_RXEMPTY) != 0;
        if no_pending_events && rx_buffer_empty {
            return;
        }
        let _ = uart_csr.rf(utra::uart::RXTX_RXTX);
        uart_csr.wfo(utra::uart::EV_PENDING_RX, 1);
    }
}

/// Receive one byte from the Debug UART, subject to RX bypass controlled by wake sequence
pub fn rx_byte(uart_state: &mut RxState) -> Option<u8> {
    let mut uart_csr = CSR::new(utra::uart::HW_UART_BASE as *mut u8);
    let no_pending_events = uart_csr.rf(utra::uart::EV_PENDING_RX) == 0;
    let rx_buffer_empty = uart_csr.rf(utra::uart::RXEMPTY_RXEMPTY) != 0;
    if no_pending_events && rx_buffer_empty {
        return None;
    }
    let b = uart_csr.rf(utra::uart::RXTX_RXTX) as u8;
    // Writing 1 to EV_PENDING_RX acts as an ACK
    uart_csr.wfo(utra::uart::EV_PENDING_RX, 1);
    // Only disable the RX bypass once "AT\n" wake sequence has been received ("AT\r\n" also works).
    // Going through the Waking -> BypassOff sequence at the end makes sure that the CR or LF char
    // that finished the wake sequence gets consumed (return None). The following char will be the
    // first Some(_).
    let next_state = match *uart_state {
        RxState::BypassOff => RxState::BypassOff,
        RxState::BypassOnAwaitA if b == b'A' => RxState::ExpectT,
        RxState::ExpectT if b == b'A' => RxState::ExpectT, // Don't get confused by AAT\n
        RxState::ExpectT if b == b'T' => RxState::ExpectCROrLF,
        RxState::ExpectCROrLF if (b == b'\r') || (b == b'\n') => RxState::Waking,
        RxState::Waking => RxState::BypassOff,
        _ => RxState::BypassOnAwaitA,
    };
    *uart_state = next_state;
    match *uart_state {
        RxState::BypassOff => Some(b),
        _ => None,
    }
}
